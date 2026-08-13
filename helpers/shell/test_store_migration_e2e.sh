#!/usr/bin/env bash
# Verifica rename, merge, split, backup offline y reconstrucción de
# cache/proyecciones en Debian.
# El contenedor es efímero; no cambia el host ni la configuración productiva.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONTAINER_NAME="${CONTAINER_NAME:-ixmati-store-migration-e2e}"
IMAGE_NAME="${IMAGE_NAME:-ixmati-store-migration-e2e}"
API_PORT="${API_PORT:-30000}"

cleanup() {
    podman rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

exec_c() {
    podman exec "$CONTAINER_NAME" bash -lc "$1"
}

wait_for_systemd() {
    for _ in $(seq 1 40); do
        state="$(exec_c 'systemctl is-system-running 2>/dev/null || true')"
        if [[ "$state" == "running" || "$state" == "degraded" ]]; then
            return 0
        fi
        sleep 1
    done
    echo "systemd no llegó a estado operativo" >&2
    return 1
}

wait_for_outbox() {
    local pending=unknown
    for _ in $(seq 1 30); do
        pending="$(exec_c "python3 -c 'import sqlite3; c=sqlite3.connect(\"/var/lib/ixmati/stores/default.db\"); print(c.execute(\"SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL\").fetchone()[0])'")"
        [[ "$pending" == "0" ]] && return 0
        sleep 1
    done
    echo "outbox no drenó: $pending" >&2
    return 1
}

echo "[migration-e2e] construyendo distribución"
make -C "$ROOT" dist dist-checksums dist-validate >/dev/null
version="$(cat "$ROOT/VERSION")"
tarball="$ROOT/dist/ixmati-${version}-linux-amd64.tar.gz"
dist_dir="ixmati-${version}-linux-amd64"
dist_path="/root/${dist_dir}/bin"

echo "[migration-e2e] construyendo imagen Debian"
podman build -t "$IMAGE_NAME" "$ROOT/containers/installer-test" >/dev/null
cleanup
podman run -d --name "$CONTAINER_NAME" --privileged \
    -v /sys/fs/cgroup:/sys/fs/cgroup:rw "$IMAGE_NAME" >/dev/null
wait_for_systemd

podman cp "$tarball" "$CONTAINER_NAME:/root/$(basename "$tarball")"
exec_c "cd /root && tar xzf $(basename "$tarball") && cd /root/$dist_dir && IXMATI_API_KEYS=ix-default-key ./install.sh"

echo "[migration-e2e] escribiendo fixture durable"
exec_c "curl -fsS -X POST http://localhost:${API_PORT}/write \
    -H 'Authorization: ApiKey ix-default-key' -H 'Content-Type: application/json' \
    -d '{\"op\":\"upsert\",\"store\":\"default\",\"entity\":\"test\",\"key\":\"k1\",\"version\":1,\"ts\":\"2026-01-01T00:00:00Z\",\"idempotency_key\":\"migration-e2e-1\",\"ack_mode\":\"committed\",\"payload\":{\"hello\":\"migration\"}}' >/dev/null"
wait_for_outbox

echo "[migration-e2e] deteniendo servicios que escriben y replican"
exec_c 'systemctl stop ixmati-api ixmati-projector ixmati-writer@default ixmati-litestream-s3 ixmati-litestream-file'

exec_c "mkdir -p /root/e2e-evidence && cat > /root/rename.toml <<'EOF'
operation = \"rename\"
hash_algorithm = \"sha256-key-v1\"
quiesced = true
evidence_dir = \"/root/e2e-evidence\"

[[sources]]
name = \"default\"
path = \"/var/lib/ixmati/stores/default.db\"

[target]
name = \"orders\"
path = \"/var/lib/ixmati/stores/orders.db\"
EOF
$dist_path/ixmati-store-migrate plan --manifest /root/rename.toml
$dist_path/ixmati-store-migrate execute --manifest /root/rename.toml
$dist_path/ixmati-store-migrate verify --manifest /root/rename.toml"

echo "[migration-e2e] reconstruyendo proyección desde el destino"
exec_c "cat > /root/e2e-projections.toml <<'EOF'
[[projections]]
name = \"orders_materialized\"
pattern = \"M\"
source_stores = [\"orders\"]
target_key = \"key\"
ttl_seconds = 300
[[projections.copy_fields]]
source_store = \"orders\"
source_entity = \"test\"
fields = [\"hello\"]
EOF
IXMATI_PROJECTIONS_PATH=/root/e2e-projections.toml \
IXMATI_STORE_PATHS=orders=/var/lib/ixmati/stores/orders.db \
CACHE_SOCKET_PATH=/var/run/ixmati/cache.sock \
$dist_path/ixmati-reconciler"

echo "[migration-e2e] reiniciando API y comprobando cache"
exec_c 'systemctl start ixmati-api'
projection=""
for _ in $(seq 1 30); do
    projection="$(exec_c "curl -fsS 'http://localhost:${API_PORT}/read?projection=orders_materialized&key=k1' -H 'Authorization: ApiKey ix-default-key' 2>/dev/null || true")"
    if grep -q '"found":true' <<<"$projection" && grep -q 'migration' <<<"$projection"; then
        echo "$projection"
        break
    fi
    sleep 1
done
grep -q '"found":true' <<<"$projection" || {
    echo "la proyección no apareció en cache: ${projection:-<sin respuesta>}" >&2
    exit 1
}

echo "[migration-e2e] verificando backup offline generado por la migración"
exec_c "python3 - <<'PY'
import pathlib
import shutil
import sqlite3

backup = pathlib.Path('/root/e2e-evidence/default.pre-migration.db')
restored = pathlib.Path('/root/restored-default.db')
assert backup.is_file(), backup
shutil.copy2(backup, restored)
with sqlite3.connect(restored) as conn:
    assert conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
    assert conn.execute('SELECT COUNT(*) FROM payload_default').fetchone()[0] == 1
    assert conn.execute('SELECT COUNT(*) FROM _idempotency').fetchone()[0] == 1
print('backup_restore=ok')
PY"

echo "[migration-e2e] creando dos stores offline para probar merge, tombstone e idempotencia"
exec_c "python3 - <<'PY'
import sqlite3
from pathlib import Path

def create(path, store, payloads, tombstones, idempotency, outbox):
    path = Path(path)
    if path.exists():
        path.unlink()
    with sqlite3.connect(path) as conn:
        conn.executescript(f'''
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE payload_{store} (
                entity TEXT NOT NULL, key TEXT NOT NULL, version INTEGER NOT NULL,
                payload BLOB NOT NULL, updated_at TEXT NOT NULL,
                PRIMARY KEY(entity, key));
            CREATE TABLE _idempotency (
                idempotency_key TEXT NOT NULL, store TEXT NOT NULL,
                entity TEXT NOT NULL, key TEXT NOT NULL, version INTEGER NOT NULL,
                operation TEXT, command_digest TEXT, applied_at TEXT NOT NULL,
                PRIMARY KEY(store, idempotency_key));
            CREATE TABLE _tombstones (
                entity TEXT NOT NULL, key TEXT NOT NULL, version INTEGER NOT NULL,
                deleted_at TEXT NOT NULL, event_id TEXT, PRIMARY KEY(entity, key));
            CREATE TABLE _outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL,
                event_type TEXT NOT NULL, store TEXT NOT NULL, entity TEXT NOT NULL,
                key TEXT NOT NULL, version INTEGER NOT NULL, occurred_at TEXT NOT NULL,
                payload BLOB NOT NULL, published_at TEXT);
        ''')
        conn.executemany(f'INSERT INTO payload_{store} VALUES (?,?,?,?,?)', payloads)
        conn.executemany('INSERT INTO _tombstones VALUES (?,?,?,?,?)', tombstones)
        conn.executemany('INSERT INTO _idempotency VALUES (?,?,?,?,?,?,?,?)', idempotency)
        conn.executemany('INSERT INTO _outbox(event_id,event_type,store,entity,key,version,occurred_at,payload,published_at) VALUES (?,?,?,?,?,?,?,?,?)', outbox)
        conn.commit()
        conn.execute('PRAGMA wal_checkpoint(TRUNCATE)')

create(
    '/root/orders-a.db', 'orders_a',
    [('pedido', 'a1', 1, b'{"source":"a"}', '2026-01-01') ,
     ('pedido', 'common', 2, b'{"source":"a","version":2}', '2026-01-02')],
    [],
    [('idem-a', 'orders_a', 'pedido', 'a1', 1, 'upsert', 'digest-a', '2026-01-01'),
     ('idem-common', 'orders_a', 'pedido', 'common', 2, 'upsert', 'digest-common', '2026-01-02')],
    [('event-a', 'upsert', 'orders_a', 'pedido', 'a1', 1, '2026-01-01', b'{"source":"a"}', '2026-01-01'),
     ('event-common-a', 'upsert', 'orders_a', 'pedido', 'common', 2, '2026-01-02', b'{"source":"a"}', '2026-01-02')],
)
create(
    '/root/orders-b.db', 'orders_b',
    [('pedido', 'b1', 1, b'{"source":"b"}', '2026-01-01')],
    [('pedido', 'common', 3, '2026-01-03', 'event-common-delete')],
    [('idem-b', 'orders_b', 'pedido', 'b1', 1, 'upsert', 'digest-b', '2026-01-01'),
     ('idem-common', 'orders_b', 'pedido', 'common', 2, 'upsert', 'digest-common', '2026-01-02')],
    [('event-b', 'upsert', 'orders_b', 'pedido', 'b1', 1, '2026-01-01', b'{"source":"b"}', '2026-01-01'),
     ('event-common-delete', 'delete', 'orders_b', 'pedido', 'common', 3, '2026-01-03', b'{"deleted":true}', '2026-01-03')],
)
print('fixtures=orders-a,orders-b')
PY
cat > /root/merge.toml <<'EOF'
operation = \"merge\"
hash_algorithm = \"sha256-key-v1\"
quiesced = true
evidence_dir = \"/root/e2e-evidence/merge\"

[[sources]]
name = \"orders_a\"
path = \"/root/orders-a.db\"

[[sources]]
name = \"orders_b\"
path = \"/root/orders-b.db\"

[target]
name = \"merged\"
path = \"/root/merged.db\"
EOF
$dist_path/ixmati-store-migrate plan --manifest /root/merge.toml
$dist_path/ixmati-store-migrate execute --manifest /root/merge.toml
$dist_path/ixmati-store-migrate verify --manifest /root/merge.toml
python3 - <<'PY'
import json
import sqlite3
from pathlib import Path
with sqlite3.connect('/root/merged.db') as conn:
    assert conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
    assert conn.execute('SELECT COUNT(*) FROM payload_merged').fetchone()[0] == 2
    assert conn.execute('SELECT COUNT(*) FROM _tombstones').fetchone()[0] == 1
    assert conn.execute('SELECT COUNT(*) FROM _idempotency').fetchone()[0] == 3
    assert conn.execute('SELECT COUNT(*) FROM _outbox').fetchone()[0] == 4
    assert conn.execute('SELECT version FROM _tombstones WHERE key=?', ('common',)).fetchone()[0] == 3
report = json.loads(Path('/root/e2e-evidence/merge/migration-report.json').read_text())
assert report['deduplicated_idempotency'] == 1, report
print('merge=ok; tombstone_wins; idempotency_deduplicated=1')
PY"

echo "[migration-e2e] verificando split reproducible en tres destinos"
exec_c "cat > /root/split.toml <<'EOF'
operation = \"split\"
hash_algorithm = \"sha256-key-v1\"
quiesced = true
evidence_dir = \"/root/e2e-evidence/split\"

[[sources]]
name = \"merged\"
path = \"/root/merged.db\"

[[targets]]
name = \"split_a\"
path = \"/root/split-a.db\"

[[targets]]
name = \"split_b\"
path = \"/root/split-b.db\"

[[targets]]
name = \"split_c\"
path = \"/root/split-c.db\"
EOF
$dist_path/ixmati-store-migrate execute --manifest /root/split.toml
$dist_path/ixmati-store-migrate verify --manifest /root/split.toml
cp /root/merged.db /root/merged-repeat.db
sed 's#/root/merged.db#/root/merged-repeat.db#; s#/root/e2e-evidence/split#/root/e2e-evidence/split-repeat#; s#split-a.db#split-a-repeat.db#; s#split-b.db#split-b-repeat.db#; s#split-c.db#split-c-repeat.db#' /root/split.toml > /root/split-repeat.toml
$dist_path/ixmati-store-migrate execute --manifest /root/split-repeat.toml
python3 - <<'PY'
import hashlib
import json
import sqlite3
from pathlib import Path

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

first = json.loads(Path('/root/e2e-evidence/split/migration-report.json').read_text())['checksums']
second = json.loads(Path('/root/e2e-evidence/split-repeat/migration-report.json').read_text())['checksums']
for name in ('split_a', 'split_b', 'split_c'):
    assert first[name] == second[name]
    path = '/root/' + name.replace('_', '-') + '.db'
    with sqlite3.connect(path) as conn:
        assert conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
        assert conn.execute('SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL').fetchone()[0] == 0
        assert sha(path) == first[name]
print('split=ok; checksums=reproducible; outbox=drained')
PY"

integrity="$(exec_c "python3 -c 'import sqlite3; c=sqlite3.connect(\"/var/lib/ixmati/stores/orders.db\"); print(c.execute(\"PRAGMA integrity_check\").fetchone()[0]); print(c.execute(\"SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL\").fetchone()[0])'")"
grep -q '^ok$' <<<"$integrity"
grep -q '^0$' <<<"$integrity"
echo "[migration-e2e] OK: rename, backup restore, merge, split reproducible, reconciler, cache, integrity y outbox verificados"

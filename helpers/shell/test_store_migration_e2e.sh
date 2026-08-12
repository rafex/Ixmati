#!/usr/bin/env bash
# Verifica rename offline y reconstrucción de cache/proyecciones en Debian.
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
exec_c "cd /root && tar xzf $(basename "$tarball") && cd /root/$dist_dir && ./install.sh"

echo "[migration-e2e] escribiendo fixture durable"
exec_c "curl -fsS -X POST http://localhost:${API_PORT}/write \
    -H 'Authorization: ApiKey ix-default-key' -H 'Content-Type: application/json' \
    -d '{\"op\":\"upsert\",\"store\":\"default\",\"entity\":\"test\",\"key\":\"k1\",\"version\":1,\"ts\":\"2026-01-01T00:00:00Z\",\"idempotency_key\":\"migration-e2e-1\",\"ack_mode\":\"committed\",\"payload\":{\"hello\":\"migration\"}}' >/dev/null"
wait_for_outbox

echo "[migration-e2e] deteniendo servicios que escriben"
exec_c 'systemctl stop ixmati-api ixmati-projector ixmati-writer@default'

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

integrity="$(exec_c "python3 -c 'import sqlite3; c=sqlite3.connect(\"/var/lib/ixmati/stores/orders.db\"); print(c.execute(\"PRAGMA integrity_check\").fetchone()[0]); print(c.execute(\"SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL\").fetchone()[0])'")"
grep -q '^ok$' <<<"$integrity"
grep -q '^0$' <<<"$integrity"
echo "[migration-e2e] OK: rename, reconciler, cache, integrity y outbox verificados"

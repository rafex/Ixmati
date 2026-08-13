#!/usr/bin/env bash
# Verifica el flujo Litestream más habitual en una instalación de un solo
# host: réplica a un volumen/ruta montada mediante file:// y restore desde esa
# misma ruta. No usa S3 ni modifica los stores del host.

set -euo pipefail

LITESTREAM_IMAGE="${LITESTREAM_IMAGE:-localhost/ixmati-litestream:local}"
DEBIAN_IMAGE="${DEBIAN_IMAGE:-docker.io/library/debian@sha256:38a76d01668772e381ad2826d876627c89e7133e2f8a0f5d567306798b0f2a16}"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
DB_VOLUME="${DB_VOLUME:-ixmati-litestream-local-db-${RUN_ID}}"
BACKUP_VOLUME="${BACKUP_VOLUME:-ixmati-litestream-local-backup-${RUN_ID}}"
RESTORE_VOLUME="${RESTORE_VOLUME:-ixmati-litestream-local-restore-${RUN_ID}}"
REPLICA_NAME="${REPLICA_NAME:-ixmati-litestream-local-replica-${RUN_ID}}"

cleanup() {
    podman rm -f "$REPLICA_NAME" >/dev/null 2>&1 || true
    podman volume rm "$DB_VOLUME" "$BACKUP_VOLUME" "$RESTORE_VOLUME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

podman image exists "$LITESTREAM_IMAGE" || {
    echo "[litestream-local] imagen no encontrada: $LITESTREAM_IMAGE" >&2
    echo "[litestream-local] constrúyela con: podman build -t $LITESTREAM_IMAGE containers/litestream" >&2
    exit 1
}

echo "[litestream-local] creando volúmenes montados"
podman volume create "$DB_VOLUME" >/dev/null
podman volume create "$BACKUP_VOLUME" >/dev/null
podman volume create "$RESTORE_VOLUME" >/dev/null

echo "[litestream-local] creando SQLite WAL con datos durable y posteriores"
podman run --rm -v "$DB_VOLUME:/data:U" "$DEBIAN_IMAGE" bash -c \
    'apt-get update -qq && apt-get install -y -qq python3 >/dev/null && python3 - <<"PY"
import sqlite3

with sqlite3.connect("/data/test.db") as conn:
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA synchronous=NORMAL")
    conn.executescript("""
      CREATE TABLE payload_default (
        entity TEXT NOT NULL, key TEXT NOT NULL, version INTEGER NOT NULL,
        payload BLOB NOT NULL, updated_at TEXT NOT NULL,
        PRIMARY KEY(entity, key)
      );
      CREATE TABLE _idempotency (
        idempotency_key TEXT NOT NULL, store TEXT NOT NULL, entity TEXT NOT NULL,
        key TEXT NOT NULL, version INTEGER NOT NULL, operation TEXT,
        command_digest TEXT, applied_at TEXT NOT NULL,
        PRIMARY KEY(store, idempotency_key)
      );
      CREATE TABLE _outbox (
        id INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL,
        event_type TEXT NOT NULL, store TEXT NOT NULL, entity TEXT NOT NULL,
        key TEXT NOT NULL, version INTEGER NOT NULL, occurred_at TEXT NOT NULL,
        payload BLOB NOT NULL, published_at TEXT
      );
    """)
    payload = "{\"value\":\"before-replication\"}"
    conn.execute("INSERT INTO payload_default VALUES (?,?,?,?,?)",
                 ("test", "local-key", 1, payload, "2026-08-13"))
    conn.execute("INSERT INTO _idempotency VALUES (?,?,?,?,?,?,?,?)",
                 ("local-idem-1", "default", "test", "local-key", 1,
                  "upsert", "digest-local-1", "2026-08-13"))
    conn.execute("""INSERT INTO _outbox
        (event_id,event_type,store,entity,key,version,occurred_at,payload,published_at)
        VALUES (?,?,?,?,?,?,?,?,?)""",
        ("local-event-1", "upsert", "default", "test", "local-key", 1,
         "2026-08-13", payload, "2026-08-13"))
    conn.commit()
PY'

echo "[litestream-local] replicando con file:///backup/test.db"
podman run -d --name "$REPLICA_NAME" --user 0 \
    -v "$DB_VOLUME:/data:U" -v "$BACKUP_VOLUME:/backup:U" \
    "$LITESTREAM_IMAGE" replicate /data/test.db file:///backup/test.db >/dev/null

replicated=0
for _ in $(seq 1 45); do
    if podman run --rm -v "$BACKUP_VOLUME:/backup:ro" "$DEBIAN_IMAGE" \
        bash -c 'find /backup -type f -print -quit | grep -q .' >/dev/null 2>&1; then
        replicated=1
        break
    fi
    sleep 1
done
if [[ "$replicated" != 1 ]]; then
    podman logs "$REPLICA_NAME" >&2 || true
    exit 1
fi

echo "[litestream-local] agregando una escritura después de la réplica inicial"
podman run --rm -v "$DB_VOLUME:/data:U" "$DEBIAN_IMAGE" bash -c \
    'apt-get update -qq && apt-get install -y -qq python3 >/dev/null && python3 - <<"PY"
import sqlite3
with sqlite3.connect("/data/test.db") as conn:
    conn.execute("UPDATE payload_default SET payload=?, version=? WHERE key=?",
                 ("{\"value\":\"after-replication\"}", 2, "local-key"))
    conn.commit()
PY'

sleep 2
podman stop "$REPLICA_NAME" >/dev/null

echo "[litestream-local] restaurando desde file:///backup/test.db"
podman run --rm --user 0 \
    -v "$BACKUP_VOLUME:/backup:ro" -v "$RESTORE_VOLUME:/restore:U" \
    "$LITESTREAM_IMAGE" restore -o /restore/restored.db file:///backup/test.db >/dev/null

podman run --rm -v "$RESTORE_VOLUME:/restore:U" "$DEBIAN_IMAGE" bash -c \
    'apt-get update -qq && apt-get install -y -qq python3 >/dev/null && python3 - <<"PY"
import sqlite3
with sqlite3.connect("/restore/restored.db") as conn:
    assert conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    assert conn.execute("SELECT payload FROM payload_default WHERE key=\"local-key\"").fetchone()[0] == "{\"value\":\"after-replication\"}"
    assert conn.execute("SELECT COUNT(*) FROM _idempotency").fetchone()[0] == 1
    assert conn.execute("SELECT COUNT(*) FROM _outbox").fetchone()[0] == 1
print("local_file_uri_restore=ok; integrity=ok; idempotency=1; outbox=1")
PY'

echo "[litestream-local] OK: réplica file:// y restore verificados"

#!/usr/bin/env bash
# Reproducible S3-compatible Litestream smoke test.
# It uses disposable Podman resources and never touches the host's stores.
set -euo pipefail

NETWORK="${NETWORK:-ixmati-litestream-s3-e2e}"
MINIO_NAME="${MINIO_NAME:-ixmati-litestream-s3-minio}"
LITESTREAM_NAME="${LITESTREAM_NAME:-ixmati-litestream-s3-replica}"
RESTORE_NAME="${RESTORE_NAME:-ixmati-litestream-s3-restore}"
MINIO_IMAGE="${MINIO_IMAGE:-quay.io/minio/minio@sha256:a1a8bd4ac40ad7881a245bab97323e18f971e4d4cba2c2007ec1bedd21cbaba2}"
MC_IMAGE="${MC_IMAGE:-quay.io/minio/mc@sha256:eb4ea9884b77704230e2423e9004d2fa738dc272876b9cc41a297d29443b8780}"
LITESTREAM_IMAGE="${LITESTREAM_IMAGE:-localhost/ixmati-litestream:local}"
DEBIAN_IMAGE="${DEBIAN_IMAGE:-docker.io/library/debian@sha256:38a76d01668772e381ad2826d876627c89e7133e2f8a0f5d567306798b0f2a16}"
BUCKET="${BUCKET:-ixmati-e2e}"
ACCESS_KEY="${MINIO_ROOT_USER:-minioadmin}"
SECRET_KEY="${MINIO_ROOT_PASSWORD:-minioadmin}"
DB_VOLUME="${DB_VOLUME:-ixmati-litestream-s3-db}"
META_VOLUME="${META_VOLUME:-ixmati-litestream-s3-meta}"
RESTORE_VOLUME="${RESTORE_VOLUME:-ixmati-litestream-s3-restore}"

cleanup() {
    podman rm -f "$LITESTREAM_NAME" "$RESTORE_NAME" "$MINIO_NAME" >/dev/null 2>&1 || true
    podman volume rm "$DB_VOLUME" "$META_VOLUME" "$RESTORE_VOLUME" >/dev/null 2>&1 || true
    podman network rm "$NETWORK" >/dev/null 2>&1 || true
    if [[ -n "${CONFIG_FILE:-}" ]]; then
        rm -f "$CONFIG_FILE"
    fi
    if [[ -n "${RESTORE_CONFIG_FILE:-}" ]]; then
        rm -f "$RESTORE_CONFIG_FILE"
    fi
}
trap cleanup EXIT

CONFIG_FILE="$(mktemp -t ixmati-litestream-s3.XXXXXX.yml)"
RESTORE_CONFIG_FILE="$(mktemp -t ixmati-litestream-s3-restore.XXXXXX.yml)"
cat >"$CONFIG_FILE" <<EOF
sync-interval: 1s
snapshot:
  retention: 1h
dbs:
  - dir: /data
    pattern: "*.db"
    watch: true
    meta-dir: /meta
    replica:
      url: s3://${BUCKET}/ixmati
      endpoint: http://${MINIO_NAME}:9000
      region: us-east-1
      sync-interval: 1s
EOF
cat >"$RESTORE_CONFIG_FILE" <<EOF
dbs:
  - path: /data/test.db
    replica:
      url: s3://${BUCKET}/ixmati/test.db
      endpoint: http://${MINIO_NAME}:9000
      region: us-east-1
EOF

echo "[litestream-s3-e2e] creando red y MinIO"
podman network create "$NETWORK" >/dev/null
podman volume create "$DB_VOLUME" >/dev/null
podman volume create "$META_VOLUME" >/dev/null
podman volume create "$RESTORE_VOLUME" >/dev/null
podman run -d --name "$MINIO_NAME" --network "$NETWORK" \
    -e MINIO_ROOT_USER="$ACCESS_KEY" \
    -e MINIO_ROOT_PASSWORD="$SECRET_KEY" \
    "$MINIO_IMAGE" server /data --address :9000 >/dev/null

echo "[litestream-s3-e2e] esperando bucket"
for _ in $(seq 1 30); do
    if podman run --rm --network "$NETWORK" \
        -e "MC_HOST_local=http://${ACCESS_KEY}:${SECRET_KEY}@${MINIO_NAME}:9000" \
        "$MC_IMAGE" mb --ignore-existing "local/${BUCKET}" >/dev/null 2>&1; then
        break
    fi
    sleep 1
done
podman run --rm --network "$NETWORK" \
    -e "MC_HOST_local=http://${ACCESS_KEY}:${SECRET_KEY}@${MINIO_NAME}:9000" \
    "$MC_IMAGE" stat "local/${BUCKET}" >/dev/null

echo "[litestream-s3-e2e] creando SQLite WAL con idempotencia y outbox"
podman run --rm --network "$NETWORK" -v "${DB_VOLUME}:/data:U" \
    "$DEBIAN_IMAGE" bash -c \
    'apt-get update -qq && apt-get install -y -qq python3 >/dev/null && python3 - <<"PY"
import sqlite3

with sqlite3.connect("/data/test.db") as conn:
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA synchronous=NORMAL")
    conn.executescript("""
      CREATE TABLE payload_default (entity TEXT NOT NULL, key TEXT NOT NULL,
        version INTEGER NOT NULL, payload BLOB NOT NULL,
        updated_at TEXT NOT NULL, PRIMARY KEY(entity,key));
      CREATE TABLE _idempotency (idempotency_key TEXT NOT NULL,
        store TEXT NOT NULL, entity TEXT NOT NULL, key TEXT NOT NULL,
        version INTEGER NOT NULL, operation TEXT, command_digest TEXT,
        applied_at TEXT NOT NULL, PRIMARY KEY(store,idempotency_key));
      CREATE TABLE _outbox (id INTEGER PRIMARY KEY AUTOINCREMENT,
        event_id TEXT NOT NULL, event_type TEXT NOT NULL, store TEXT NOT NULL,
        entity TEXT NOT NULL, key TEXT NOT NULL, version INTEGER NOT NULL,
        occurred_at TEXT NOT NULL, payload BLOB NOT NULL, published_at TEXT);
    """)
    payload = "{\"value\":\"before-replication\"}"
    conn.execute("INSERT INTO payload_default VALUES (?,?,?,?,?)",
                 ("test", "s3-key", 1, payload, "2026-08-13"))
    conn.execute("INSERT INTO _idempotency VALUES (?,?,?,?,?,?,?,?)",
                 ("s3-idem-1", "default", "test", "s3-key", 1,
                  "upsert", "digest-1", "2026-08-13"))
    conn.execute("""INSERT INTO _outbox
        (event_id,event_type,store,entity,key,version,occurred_at,payload,published_at)
        VALUES (?,?,?,?,?,?,?,?,?)""",
        ("s3-event-1", "upsert", "default", "test", "s3-key", 1,
         "2026-08-13", payload, "2026-08-13"))
    conn.commit()
PY'

echo "[litestream-s3-e2e] iniciando replicación"
podman create --name "$LITESTREAM_NAME" --network "$NETWORK" --user 0 \
    -e AWS_ACCESS_KEY_ID="$ACCESS_KEY" \
    -e AWS_SECRET_ACCESS_KEY="$SECRET_KEY" \
    -e AWS_REGION=us-east-1 \
    -v "${DB_VOLUME}:/data:U" -v "${META_VOLUME}:/meta:U" \
    "$LITESTREAM_IMAGE" replicate -config /etc/litestream-s3.yml >/dev/null
podman cp "$CONFIG_FILE" "$LITESTREAM_NAME:/etc/litestream-s3.yml"
podman start "$LITESTREAM_NAME" >/dev/null

replicated=0
for _ in $(seq 1 45); do
    if [[ "$(podman run --rm --network "$NETWORK" \
        -e "MC_HOST_local=http://${ACCESS_KEY}:${SECRET_KEY}@${MINIO_NAME}:9000" \
        "$MC_IMAGE" ls --recursive "local/${BUCKET}" 2>/dev/null | wc -l | tr -d ' ')" -gt 0 ]]; then
        replicated=1
        break
    fi
    sleep 1
done
[[ "$replicated" == 1 ]] || {
    podman logs "$LITESTREAM_NAME" >&2 || true
    exit 1
}
podman run --rm --network "$NETWORK" \
    -e "MC_HOST_local=http://${ACCESS_KEY}:${SECRET_KEY}@${MINIO_NAME}:9000" \
    "$MC_IMAGE" ls --recursive "local/${BUCKET}"

echo "[litestream-s3-e2e] restaurando desde S3"
podman create --name "$RESTORE_NAME" --network "$NETWORK" --user 0 \
    -e AWS_ACCESS_KEY_ID="$ACCESS_KEY" \
    -e AWS_SECRET_ACCESS_KEY="$SECRET_KEY" \
    -e AWS_REGION=us-east-1 \
    -v "${DB_VOLUME}:/data:ro" -v "${RESTORE_VOLUME}:/restore:U" \
    "$LITESTREAM_IMAGE" restore -config /etc/litestream-s3.yml \
    -o /restore/restored.db /data/test.db >/dev/null
podman cp "$RESTORE_CONFIG_FILE" "${RESTORE_NAME}:/etc/litestream-s3.yml"
podman start --attach "$RESTORE_NAME"
podman run --rm -v "${RESTORE_VOLUME}:/restore:U" "$DEBIAN_IMAGE" \
    bash -c \
    'apt-get update -qq && apt-get install -y -qq python3 >/dev/null && python3 - <<"PY"
import sqlite3
with sqlite3.connect("/restore/restored.db") as conn:
    assert conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    assert conn.execute("SELECT payload FROM payload_default WHERE key=\"s3-key\"").fetchone()[0]
    assert conn.execute("SELECT COUNT(*) FROM _idempotency").fetchone()[0] == 1
print("s3_restore=ok; integrity=ok; idempotency=1")
PY'

echo "[litestream-s3-e2e] OK: replica S3-compatible y restore verificados"

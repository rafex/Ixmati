#!/usr/bin/env bash
# helpers/shell/litestream_restore.sh — restauracion de un store desde Litestream

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

STORE="${1:-}"
OUTPUT="${2:-}"
TIMESTAMP="${3:-}"

if [ -z "$STORE" ] || [ -z "$OUTPUT" ]; then
    echo "uso: $0 <store> <output_path> [timestamp]"
    echo "  store:    nombre del store (ej. pedidos)"
    echo "  output:   ruta del archivo restaurado (ej. /data/pedidos_restored.db)"
    echo "  timestamp: punto en el tiempo (ISO 8601, opcional)"
    exit 1
fi

LITESTREAM_BIN="${IXMATI_LITESTREAM_BIN:-/usr/local/lib/ixmati/litestream}"
if [ ! -x "$LITESTREAM_BIN" ]; then
    if command -v litestream >/dev/null 2>&1; then
        LITESTREAM_BIN="$(command -v litestream)"
    else
        die "Litestream no encontrado; ejecuta el instalador nativo o define IXMATI_LITESTREAM_BIN"
    fi
fi

BACKUP_DIR="${IXMATI_LITESTREAM_BACKUP_DIR:-/var/lib/ixmati/backups}"
S3_BUCKET="${IXMATI_LITESTREAM_S3_BUCKET:-${LITESTREAM_S3_BUCKET:-}}"
S3_PREFIX="${IXMATI_LITESTREAM_S3_PREFIX:-${LITESTREAM_S3_PREFIX:-ixmati}}"
if [ -n "${LITESTREAM_REPLICA_URL:-}" ]; then
    REPLICA_URL="$LITESTREAM_REPLICA_URL"
elif [ -n "${LITESTREAM_S3_URL:-}" ]; then
    REPLICA_URL="$LITESTREAM_S3_URL"
elif [ -n "$S3_BUCKET" ]; then
    if [ -n "$S3_PREFIX" ]; then
        REPLICA_URL="s3://${S3_BUCKET%/}/${S3_PREFIX%/}/${STORE}.db"
    else
        REPLICA_URL="s3://${S3_BUCKET%/}/${STORE}.db"
    fi
elif [ -e "${BACKUP_DIR}/${STORE}.db" ]; then
    # Native installs use Litestream's directory watcher. The per-store
    # replica URL is the database path inside that directory.
    REPLICA_URL="file://${BACKUP_DIR}/${STORE}.db"
else
    die "no hay réplica configurada para ${STORE}; define LITESTREAM_REPLICA_URL, LITESTREAM_S3_URL, LITESTREAM_S3_BUCKET o restaura desde ${BACKUP_DIR}/${STORE}.db"
fi

log "restaurando store=$STORE desde $REPLICA_URL..."

RESTORE_CONFIG_ARGS=()
if [[ "$REPLICA_URL" == s3://* ]]; then
    S3_CONFIG="${IXMATI_LITESTREAM_S3_CONFIG:-/etc/ixmati/litestream-s3.yml}"
    if [ -f "$S3_CONFIG" ]; then
        RESTORE_CONFIG_ARGS=(-config "$S3_CONFIG")
    fi
fi

if [ -n "$TIMESTAMP" ]; then
    log "punto en el tiempo: $TIMESTAMP"
    "$LITESTREAM_BIN" restore "${RESTORE_CONFIG_ARGS[@]}" -o "$OUTPUT" --timestamp "$TIMESTAMP" "$REPLICA_URL"
else
    log "ultimo backup disponible"
    "$LITESTREAM_BIN" restore "${RESTORE_CONFIG_ARGS[@]}" -o "$OUTPUT" "$REPLICA_URL"
fi

ok "restauracion completa: $OUTPUT"

log "verificando integridad..."
sqlite3 "$OUTPUT" "PRAGMA integrity_check;" >/dev/null && ok "integridad OK" || die "integridad FALLIDA"

log "restauracion exitosa"

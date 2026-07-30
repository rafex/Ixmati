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

require litestream "instala: brew install litestream"

S3_URL="${LITESTREAM_S3_URL:-s3://ixmati-backups/$STORE}"

log "restaurando store=$STORE desde $S3_URL..."

if [ -n "$TIMESTAMP" ]; then
    log "punto en el tiempo: $TIMESTAMP"
    litestream restore -o "$OUTPUT" --timestamp "$TIMESTAMP" "$S3_URL"
else
    log "ultimo backup disponible"
    litestream restore -o "$OUTPUT" "$S3_URL"
fi

ok "restauracion completa: $OUTPUT"

log "verificando integridad..."
sqlite3 "$OUTPUT" "PRAGMA integrity_check;" >/dev/null && ok "integridad OK" || die "integridad FALLIDA"

log "restauracion exitosa"

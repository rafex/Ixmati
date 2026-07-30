#!/usr/bin/env bash
# helpers/shell/sqlite_integrity.sh — verificacion de integridad de un store SQLite

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

DB="${1:-}"

if [ -z "$DB" ]; then
    echo "uso: $0 <ruta_a_store.db>"
    exit 1
fi

if [ ! -f "$DB" ]; then
    die "archivo no encontrado: $DB"
fi

log "verificando integridad de $DB"

# integridad completa
RESULT="$(sqlite3 "$DB" "PRAGMA integrity_check;" 2>&1)"
if [ "$RESULT" = "ok" ]; then
    ok "integrity_check: ok"
else
    die "integrity_check: $RESULT"
fi

# chequeo rapido
RESULT="$(sqlite3 "$DB" "PRAGMA quick_check;" 2>&1)"
if [ "$RESULT" = "ok" ]; then
    ok "quick_check: ok"
else
    err "quick_check: $RESULT"
fi

# tamanio
SIZE="$(stat -f%z "$DB" 2>/dev/null || stat -c%s "$DB" 2>/dev/null)"
ok "tamanio: $SIZE bytes"

# modo WAL
JOURNAL="$(sqlite3 "$DB" "PRAGMA journal_mode;" 2>&1)"
if [ "$JOURNAL" = "wal" ]; then
    ok "journal_mode: wal"
else
    warn "journal_mode: $JOURNAL (se recomienda wal)"
fi

log "verificacion completa"

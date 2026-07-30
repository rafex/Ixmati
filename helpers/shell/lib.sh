#!/usr/bin/env bash
# helpers/shell/lib.sh — funciones compartidas para todos los scripts

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

# colores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

log()    { echo -e "${BLUE}[$(date +'%H:%M:%S')]${NC} $*"; }
ok()     { echo -e "  ${GREEN}✓${NC} $*"; }
warn()   { echo -e "  ${YELLOW}⚠${NC} $*"; }
err()    { echo -e "  ${RED}✗${NC} $*"; }

die() {
    echo -e "${RED}[FATAL]${NC} $*" >&2
    exit 1
}

require() {
    local tool="$1"
    local hint="${2:-}"
    if ! command -v "$tool" &>/dev/null; then
        err "$tool no encontrado"
        if [ -n "$hint" ]; then
            echo -e "  ${YELLOW}$hint${NC}"
        fi
        exit 1
    fi
    ok "$tool encontrado"
}

require_version() {
    local tool="$1"
    local min="$2"
    local version_cmd="${3:-$tool --version}"
    local actual

    if ! command -v "$tool" &>/dev/null; then
        err "$tool no encontrado"
        return 1
    fi

    actual="$($version_cmd 2>/dev/null | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)"
    if [ -z "$actual" ]; then
        warn "no se pudo detectar version de $tool"
        return 0
    fi

    if [ "$(printf '%s\n' "$min" "$actual" | sort -V | head -1)" != "$min" ]; then
        warn "$tool $actual < $min (minimo)"
        return 1
    fi
    ok "$tool $actual (>= $min)"
}

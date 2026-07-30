#!/usr/bin/env bash
# .githooks/_common.sh — resolucion de paths para hooks

GIT_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "$(dirname "$0")/..")"
HELPERS_SHELL="$GIT_ROOT/helpers/shell"

# source lib.sh si existe
if [ -f "$HELPERS_SHELL/lib.sh" ]; then
    source "$HELPERS_SHELL/lib.sh"
fi

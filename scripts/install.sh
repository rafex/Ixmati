#!/usr/bin/env bash
# Ixmati installer — wrapper minimo
# Uso offline: sudo ./install.sh
# Uso online:  curl -sSL <url>/install.sh | sudo bash
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "Ejecuta como root: sudo ./install.sh"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ -f "$SCRIPT_DIR/installer.py" ]; then
    python3 "$SCRIPT_DIR/installer.py" "$@"
elif [ -f "$SCRIPT_DIR/helpers/python/installer.py" ]; then
    python3 "$SCRIPT_DIR/helpers/python/installer.py" "$@"
else
    echo "[ERROR] installer.py no encontrado"
    exit 1
fi

#!/usr/bin/env bash
# Ixmati installer — wrapper minimo
# Uso offline: sudo ./install.sh
# Uso online:  curl -sSL <url>/install.sh | sudo bash
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "Ejecuta como root: sudo ./install.sh"
    exit 1
fi

if ! command -v python3 &>/dev/null; then
    echo "[ixmati] python3 no encontrado, instalando..."
    if command -v apt-get &>/dev/null; then
        apt-get update -qq && apt-get install -y -qq python3
    elif command -v dnf &>/dev/null; then
        dnf install -y python3
    elif command -v yum &>/dev/null; then
        yum install -y python3
    else
        echo "[ERROR] no se detectó apt-get, dnf ni yum. Instala python3 manualmente."
        exit 1
    fi
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

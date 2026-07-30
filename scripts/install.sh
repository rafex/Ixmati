#!/usr/bin/env bash
# Ixmati installer — wrapper bash mínimo
# Uso:
#   curl -sSL https://raw.githubusercontent.com/rafex/Ixmati/main/scripts/install.sh | bash
#   ./install.sh --version 0.1.0 --prefix /usr/local --no-systemd
set -euo pipefail

REPO="rafex/Ixmati"
VERSION="${IXMATI_VERSION:-latest}"
export IXMATI_VERSION="$VERSION"

# ── instalar uv si no existe ──
if ! command -v uv &>/dev/null; then
    echo "[installer] instalando uv..."
    curl -LsSf https://astral.sh/uv/install.sh | sh
    export PATH="$HOME/.local/bin:$PATH"
fi

echo "[installer] Ixmati v${VERSION}"
echo ""

# ── modo offline: installer.py ya está en el tar.gz ──
if [ -f "installer.py" ]; then
    uv run installer.py "$@"
else
    uv run --with click --from "git+https://github.com/${REPO}" installer.py "$@"
fi

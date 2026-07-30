#!/usr/bin/env bash
# helpers/shell/preflight.sh — verifica que el entorno de desarrollo este listo

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

log "Ixmati — preflight check"

echo ""
log "herramientas requeridas:"
require rustup   "instala: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
require cargo    ""
require rustc    ""
require just     "instala: brew install just  o  cargo install just"
require make     ""
require uv       "instala: curl -LsSf https://astral.sh/uv/install.sh | sh"
require python3  "uv gestiona Python >= 3.12"
require protoc   "instala: brew install protobuf"
require sqlite3  ""
require docker   "instala: https://docs.docker.com/get-docker/"

echo ""
log "versiones:"
require_version rustc  "1.80"  "rustc --version"
require_version just   "1.0"   "just --version"
require_version uv     "0.3"   "uv --version"
require_version protoc "25.0"  "protoc --version"

echo ""
log "Python (uv):"
uv python find 3.12 2>/dev/null && ok "Python 3.12 disponible via uv" || warn "instalando Python 3.12 con uv..." && uv python install 3.12

echo ""
log "herramientas opcionales:"
require mosquitto "instala: brew install mosquitto" || warn "solo necesario para desarrollo local con docker"
require mdbook    "instala: cargo install mdbook" || warn "solo necesario para generar docs"
require cargo-llvm-cov "instala: cargo install cargo-llvm-cov" || warn "solo necesario para coverage gate"
require cargo-deny     "instala: cargo install cargo-deny" || warn "solo necesario para auditoria de dependencias"
require cargo-audit    "instala: cargo install cargo-audit" || warn "solo necesario para auditoria de dependencias"

echo ""
log "servicios:"
docker compose -f "$REPO_ROOT/docker/docker-compose.dev.yml" config --quiet 2>/dev/null && ok "docker compose config valido" || warn "docker compose no configurado"

echo ""
log "preflight completo."

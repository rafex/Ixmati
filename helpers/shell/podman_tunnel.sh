#!/usr/bin/env bash
# helpers/shell/podman_tunnel.sh — gestiona el tunel SSH al podman remoto
#
# up:     levanta el tunel si no existe
# down:   lo derriba
# status: reporta si esta activo

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

SSH_HOST="rafex@192.168.3.175"
TUNNEL_PORT="18081"
REMOTE_SOCK="/run/user/1000/podman/podman.sock"
TUNNEL_SPEC="127.0.0.1:${TUNNEL_PORT}:${REMOTE_SOCK}"

check_tunnel_up() {
    nc -z 127.0.0.1 "$TUNNEL_PORT" 2>/dev/null
}

case "${1:-status}" in
    up)
        if check_tunnel_up; then
            ok "tunel ya activo en :${TUNNEL_PORT}"
            exit 0
        fi
        log "levantando tunel SSH a ${SSH_HOST}..."
        ssh -fN -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 \
            -L "$TUNNEL_SPEC" "$SSH_HOST"
        sleep 1
        if check_tunnel_up; then
            ok "tunel activo en :${TUNNEL_PORT}"
        else
            die "no se pudo establecer el tunel"
        fi
        ;;
    down)
        log "derribando tunel en :${TUNNEL_PORT}..."
        TUNNEL_PID=$(lsof -nP -iTCP:"$TUNNEL_PORT" -sTCP:LISTEN -t 2>/dev/null || true)
        if [ -n "$TUNNEL_PID" ]; then
            kill "$TUNNEL_PID" 2>/dev/null || true
            ok "tunel detenido (PID=$TUNNEL_PID)"
        else
            ok "tunel no estaba activo"
        fi
        ;;
    status)
        if check_tunnel_up; then
            TUNNEL_PID=$(lsof -nP -iTCP:"$TUNNEL_PORT" -sTCP:LISTEN -t 2>/dev/null || echo "?")
            ok "tunel activo en :${TUNNEL_PORT} (PID=${TUNNEL_PID})"
        else
            warn "tunel inactivo. Ejecuta: podman-tunnel-up"
            exit 1
        fi
        ;;
    *)
        echo "uso: $0 {up|down|status}"
        exit 1
        ;;
esac

#!/usr/bin/env bash
# helpers/shell/podman_remote.sh — valida conexion al podman remoto
#
# Verifica que:
# 1. El podman esta operativo en la conexion default
# 2. El target es amd64 (aborta si no)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

log "verificando conexion a podman remoto..."

# verificar que el tunel esta activo
python3 -c "
import urllib.request
try:
    resp = urllib.request.urlopen('http://127.0.0.1:18081/_ping', timeout=5)
    assert resp.status == 200
except Exception as e:
    print(f'FALLIDO: {e}')
    exit(1)
print('OK')
" 2>/dev/null || die "no se puede conectar al podman remoto en :18081. Ejecuta: podman-tunnel-up"

ok "podman responde en :18081"

# verificar arquitectura del host remoto
ARCH=$(python3 -c "
import urllib.request, json
try:
    resp = urllib.request.urlopen('http://127.0.0.1:18081/v5.0.0/libpod/info', timeout=5)
    data = json.loads(resp.read())
    arch = data['host']['arch']
    print(arch)
except Exception as e:
    print(f'ERROR: {e}')
    exit(1)
" 2>/dev/null)

if [ "$ARCH" != "amd64" ]; then
    die "target es ${ARCH}, se espera amd64. Aborta para no construir en la maquina equivocada."
fi

ok "target: amd64"
ok "conexion podman remoto OK"

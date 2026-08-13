#!/usr/bin/env bash
# helpers/shell/test_installer_debian.sh — valida el instalador nativo
# (scripts/install.sh + helpers/python/installer.py) dentro de un contenedor
# Debian con systemd real como PID 1.
#
# Requiere Podman con soporte --privileged (cgroups montados). No requiere
# el tunel al podman remoto: corre contra la conexion podman por defecto.
#
# Pasos:
#   1. make dist && make dist-checksums && make dist-validate
#   2. build de containers/installer-test (Debian + systemd)
#   3. levanta el contenedor, copia el tarball, corre install.sh
#   4. verifica que los 5 servicios queden active
#   5. round-trip funcional: /health, POST /write, GET /read
#   6. segunda pasada de install.sh (idempotencia) sin romper nada
#   7. install.sh --uninstall --purge y confirma limpieza
#
# Uso: helpers/shell/test_installer_debian.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

CONTAINER_NAME="ixmati-installer-test"
IMAGE_NAME="ixmati-installer-test"
API_PORT="30000"

cleanup() {
    log "limpiando contenedor de test..."
    podman rm -f "$CONTAINER_NAME" &>/dev/null || true
}
trap cleanup EXIT

exec_c() {
    podman exec "$CONTAINER_NAME" bash -c "$1"
}

wait_for_systemd() {
    log "esperando a que systemd (PID 1) este listo..."
    local elapsed=0
    while [ "$elapsed" -lt 30 ]; do
        if exec_c "systemctl is-system-running --wait" &>/dev/null; then
            ok "systemd listo"
            return 0
        fi
        # "degraded" tambien es aceptable (unidades no relacionadas con ixmati)
        state="$(exec_c "systemctl is-system-running" 2>/dev/null || true)"
        if [ "$state" = "degraded" ] || [ "$state" = "running" ]; then
            ok "systemd listo (${state})"
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    die "systemd no llego a estado operativo tras ${elapsed}s"
}

check_services_active() {
    local label="$1"
    log "verificando servicios (${label})..."
    local services=(mosquitto ixmati-cache-server "ixmati-writer@default" ixmati-api ixmati-projector)
    local fail=0
    for svc in "${services[@]}"; do
        status="$(exec_c "systemctl is-active ${svc}" 2>/dev/null || echo inactive)"
        if [ "$status" = "active" ]; then
            ok "${svc} → active"
        else
            err "${svc} → ${status}"
            fail=1
        fi
    done
    [ "$fail" -eq 0 ] || die "servicios no activos tras ${label}"
}

check_write_read_roundtrip() {
    local version="${1:-1}"
    log "probando round-trip write/read (version=${version})..."
    local idem_key="installer-test-${version}-$(date +%s)"
    local write_resp
    write_resp="$(exec_c "curl -sS -X POST http://localhost:${API_PORT}/write \
        -H 'Authorization: ApiKey ix-default-key' \
        -H 'Content-Type: application/json' \
        -d '{\"op\":\"upsert\",\"store\":\"default\",\"entity\":\"test\",\"key\":\"k1\",\"version\":${version},\"ts\":\"2026-01-01T00:00:00Z\",\"idempotency_key\":\"${idem_key}\",\"ack_mode\":\"committed\",\"payload\":{\"hello\":\"world\"}}'")"
    echo "$write_resp" | grep -q "\"$idem_key\"" || die "write falló: $write_resp"
    ok "POST /write → $write_resp"

    local read_resp
    read_resp="$(exec_c "curl -sS 'http://localhost:${API_PORT}/read?store=default&entity=test&key=k1' \
        -H 'Authorization: ApiKey ix-default-key'")"
    echo "$read_resp" | grep -q '"hello":"world"' || die "read no devolvió el payload esperado: $read_resp"
    ok "GET /read → $read_resp"
}

log "=== 1/7: empaquetando dist/ ==="
make dist
make dist-checksums
make dist-validate

VERSION="$(cat VERSION 2>/dev/null || echo 0.0.0)"
TARBALL="dist/ixmati-${VERSION}-linux-amd64.tar.gz"
[ -f "$TARBALL" ] || die "tarball no encontrado: $TARBALL"

log "=== 2/7: build de la imagen de test (Debian + systemd) ==="
podman build -t "$IMAGE_NAME" containers/installer-test

log "=== 3/7: levantando contenedor privilegiado ==="
cleanup
podman run -d --name "$CONTAINER_NAME" --privileged \
    -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
    "$IMAGE_NAME"
wait_for_systemd

log "=== 4/7: instalando Ixmati dentro del contenedor ==="
podman cp "$TARBALL" "$CONTAINER_NAME:/root/$(basename "$TARBALL")"
exec_c "cd /root && tar xzf $(basename "$TARBALL")"
DIST_DIRNAME="ixmati-${VERSION}-linux-amd64"
exec_c "cd /root/${DIST_DIRNAME} && IXMATI_API_KEYS=ix-default-key ./install.sh"

check_services_active "instalación limpia"
check_write_read_roundtrip

log "=== 5/7: verificando idempotencia (segunda pasada) ==="
exec_c "cd /root/${DIST_DIRNAME} && ./install.sh"
check_services_active "reinstalación"
check_write_read_roundtrip 2

log "=== 6/7: desinstalando (--uninstall --purge) ==="
exec_c "cd /root/${DIST_DIRNAME} && ./install.sh --uninstall --purge"

log "verificando limpieza..."
for svc in ixmati-cache-server "ixmati-writer@default" ixmati-api ixmati-projector; do
    status="$(exec_c "systemctl is-active ${svc}" 2>/dev/null || echo inactive)"
    [ "$status" != "active" ] || die "${svc} sigue activo tras uninstall"
    ok "${svc} → ${status}"
done
exec_c "test ! -d /var/lib/ixmati" && ok "/var/lib/ixmati eliminado" || die "/var/lib/ixmati no fue purgado"
exec_c "test ! -f /usr/local/bin/ixmati-api" && ok "binarios eliminados" || die "binarios no fueron eliminados"

log "=== 7/7: OK ==="
ok "instalador validado en debian:trixie-slim: instalación limpia, idempotencia y desinstalación funcionan"

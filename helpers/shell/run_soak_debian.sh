#!/usr/bin/env bash
# Provisiona un Debian efímero por escalón y ejecuta el soak rate-controlled.
# Requiere una conexión Podman al host Debian de validación.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TEST_HOST="${TEST_HOST:-192.168.3.175}"
API_PORT="${API_PORT:-30300}"
WRITER_METRICS_PORT="${WRITER_METRICS_PORT:-30301}"
IMAGE_NAME="${IMAGE_NAME:-ixmati-soak-debian}"
CONTAINER_PREFIX="${CONTAINER_PREFIX:-ixmati-soak}"
SOAK_DURATION="${DURATION:-3600}"
DRAIN_SECONDS="${DRAIN_SECONDS:-300}"
CONCURRENCY="${CONCURRENCY:-200}"
RATES=(${SOAK_RATES:-150 200})

cleanup_container() {
    local container="$1"
    podman rm -f "$container" >/dev/null 2>&1 || true
}

wait_for_systemd() {
    local container="$1"
    for _ in $(seq 1 40); do
        state="$(podman exec "$container" systemctl is-system-running 2>/dev/null || true)"
        if [[ "$state" == "running" || "$state" == "degraded" ]]; then
            return 0
        fi
        sleep 1
    done
    echo "systemd no llegó a estado operativo en $container" >&2
    return 1
}

echo "[soak] construyendo distribución e imagen Debian"
make -C "$ROOT" dist dist-checksums dist-validate >/dev/null
podman build -t "$IMAGE_NAME" "$ROOT/containers/installer-test" >/dev/null
version="$(cat "$ROOT/VERSION")"
tarball="$ROOT/dist/ixmati-${version}-linux-amd64.tar.gz"
dist_dir="ixmati-${version}-linux-amd64"

for rate in "${RATES[@]}"; do
    container="${CONTAINER_PREFIX}-${rate}"
    evidence="$ROOT/spec-native/evidence/raw/soak-${rate}-$(date -u +%Y%m%dT%H%M%SZ)"
    cleanup_container "$container"
    trap 'cleanup_container "$container"' EXIT INT TERM

    echo "[soak] preparando contenedor=$container rate=${rate}/s"
    podman run -d --name "$container" --privileged \
        -p "${API_PORT}:30000" -p "${WRITER_METRICS_PORT}:30301" \
        -v /sys/fs/cgroup:/sys/fs/cgroup:rw "$IMAGE_NAME" >/dev/null
    wait_for_systemd "$container"
    podman cp "$tarball" "$container:/root/$(basename "$tarball")"
    podman exec "$container" bash -lc \
        "cd /root && tar xzf $(basename "$tarball") && cd /root/$dist_dir && ./install.sh"

    podman exec "$container" bash -lc 'mkdir -p /etc/systemd/system/ixmati-writer@default.service.d
cat > /etc/systemd/system/ixmati-writer@default.service.d/metrics.conf <<EOF
[Service]
Environment=METRICS_PORT=30301
EOF
systemctl daemon-reload
systemctl restart ixmati-writer@default'

    TEST_HOST="$TEST_HOST" API_PORT="$API_PORT" \
        WRITER_METRICS_PORT="$WRITER_METRICS_PORT" \
        CONTAINER_NAME="$container" DURATION="$SOAK_DURATION" \
        DRAIN_SECONDS="$DRAIN_SECONDS" CONCURRENCY="$CONCURRENCY" \
        SOAK_RATES="$rate" OUT_DIR="$evidence" \
        "$ROOT/benchmarks/soak_capacity.sh"

    cleanup_container "$container"
    trap - EXIT INT TERM
done

echo "[soak] escenarios completados; evidencia en spec-native/evidence/raw/"

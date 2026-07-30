# helpers/just/containers.just — tareas de contenedores

repo_root := `git rev-parse --show-toplevel 2>/dev/null || pwd`

# ── tunnel ──

# levanta el tunel SSH al podman remoto
[group('containers')]
podman-tunnel-up:
    @{{repo_root}}/helpers/shell/podman_tunnel.sh up

# derriba el tunel
[group('containers')]
podman-tunnel-down:
    @{{repo_root}}/helpers/shell/podman_tunnel.sh down

# estado del tunel
[group('containers')]
podman-tunnel-status:
    @{{repo_root}}/helpers/shell/podman_tunnel.sh status

# verifica conexion al podman remoto (arquitectura, API)
[group('containers')]
podman-check:
    @{{repo_root}}/helpers/shell/podman_remote.sh

# ── build ──

# construye todas las imagenes
[group('containers')]
containers-build:
    @make containers-build

# construye solo el builder compartido
[group('containers')]
containers-builder:
    @make containers-builder

# costruye solo imagenes de infra (mosquitto, litestream)
[group('containers')]
containers-infra:
    @make containers-infra

# ── compose ──

# levanta entorno de desarrollo (mosquitto)
[group('containers')]
containers-dev:
    @just podman-tunnel-status
    @COMPOSE_FILE=dev.yaml make containers-up

# levanta entorno de test (mosquitto + api, rango Temp)
[group('containers')]
containers-test:
    @just podman-tunnel-status
    @COMPOSE_FILE=test.yaml make containers-up

# levanta stack single-store
[group('containers')]
containers-single:
    @just podman-tunnel-status
    @COMPOSE_FILE=single-store.yaml make containers-up

# levanta stack multi-store
[group('containers')]
containers-multi:
    @just podman-tunnel-status
    @COMPOSE_FILE=multi-store.yaml make containers-up

# detiene el compose activo
[group('containers')]
containers-down file="dev":
    @COMPOSE_FILE={{file}}.yaml make containers-down

# logs del compose activo
[group('containers')]
containers-logs file="dev" service="":
    podman compose -f {{repo_root}}/containers/compose/{{file}}.yaml logs -f {{service}}

# ── quadlet ──

# instala unidades quadlet (copia a ~/.config/containers/systemd/)
[group('containers')]
quadlet-install:
    @echo "[quadlet] instalando unidades..."
    cp {{repo_root}}/containers/quadlet/*{.network,.volume,.container} {{env_var_or("HOME", "/home/rafex")}}/.config/containers/systemd/
    systemctl --user daemon-reload
    @echo "[quadlet] listo. Unidades disponibles:"
    @ls {{env_var_or("HOME", "/home/rafex")}}/.config/containers/systemd/ixmati-*

# estado de las unidades quadlet
[group('containers')]
quadlet-status:
    systemctl --user status 'ixmati-*' || echo "ninguna unidad activa"

# ── limpieza ──

[group('containers')]
containers-clean:
    @make containers-clean

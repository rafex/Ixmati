# helpers/just/dev.just — tareas de desarrollo local

# levanta servicios de desarrollo (Mosquitto, etc.)
[group('dev')]
env-up:
    @echo "[env] levantando servicios de desarrollo..."
    docker compose -f docker/docker-compose.dev.yml up -d
    @{{repo_root}}/helpers/shell/wait_for.sh localhost 1883 10 "Mosquitto"

# detiene servicios de desarrollo
[group('dev')]
env-down:
    @echo "[env] deteniendo servicios..."
    docker compose -f docker/docker-compose.dev.yml down

# logs de servicios de desarrollo
[group('dev')]
logs service="":
    @docker compose -f docker/docker-compose.dev.yml logs -f {{service}}

# watch: recompila y ejecuta tests al cambiar archivos
[group('dev')]
watch:
    @cargo watch -x check -x test

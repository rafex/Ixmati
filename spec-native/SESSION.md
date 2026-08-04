+++
[session]
state = "in_progress"
agent = "unknown"
initiative = "smoke-tests"
task = "TASK-SMOKE-0001"
intent = "Ejecutar smoke tests (standalone + E2E). Standalone 4/4 pass. E2E requiere stack compose en podman remoto. Se corrigieron múltiples bugs que impedían levantar el stack y conectar desde local."
last_updated = "2026-08-04T14:52:06Z"
+++

# Active Session

## Current state

Ejecutar smoke tests (standalone + E2E). Standalone 4/4 pass. E2E requiere stack compose en podman remoto. Se corrigieron múltiples bugs que impedían levantar el stack y conectar desde local.

## Next steps

1. Verificar que el stack compose sigue UP: podman compose -f containers/compose/smoke.yaml ps
2. Verificar que los SSH port forwards siguen activos: lsof -nP -iTCP:30310 -sTCP:LISTEN
3. Si el stack no está UP, ejecutar: podman compose -f containers/compose/smoke.yaml up -d
4. Si el port forward no está activo: ssh -fN -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 30310:localhost:30310 -L 30311:localhost:30311 rafex@192.168.3.175
5. Ejecutar los tests E2E: cd helpers/python && SMOKE_HOST=localhost PYTHONPATH=$(git rev-parse --show-toplevel) uv run pytest $(git rev-parse --show-toplevel)/tests/smoke/ -v --tb=short
6. Revisar resultados y corregir fallos si los hay
7. Derribar stack al terminar: podman compose -f containers/compose/smoke.yaml down -v
8. Limpiar port forwards al terminar: kill $(lsof -nP -iTCP:30310 -sTCP:LISTEN -t 2>/dev/null) $(lsof -nP -iTCP:30311 -sTCP:LISTEN -t 2>/dev/null) 2>/dev/null; true
9. Reportar resultados finales

## Context for next agent

## Entorno
- Podman remoto en 192.168.3.175 vía túnel SSH (puerto 18081, socket podman)
- Imágenes construidas: ixmati-builder, ixmati-api, ixmati-writer, ixmati-projector, ixmati-supervisor, ixmati-reconciler, ixmati-mosquitto, ixmati-litestream (todas tag :local)
- Python venv en helpers/python/.venv con pytest, paho-mqtt, jsonschema
- SSH port forwards activos: localhost:30310→remote:30310 (mosquitto), localhost:30311→remote:30311 (api)

## Archivos modificados
1. `.containerignore` — NUEVO. Excluye target/, .git/, .venv/, __pycache__/, node_modules/, dist/, *.db*, .idea/, .vscode/, .DS_Store, *.log. Sin esto el build enviaba ~5GB por túnel SSH.
2. `containers/compose/smoke.yaml` — Build contexts de mosquitto y litestream corregidos (../../ → ../mosquitto y ../litestream). Healthcheck de mosquitto cambiado de $SYS/broker/version (deprecado en 2.0) a mosquitto_pub directo.
3. `tests/smoke/conftest.py` — Eliminado --build del compose up (imágenes pre-construidas). Añadido _resolve_smoke_host() que lee SMOKE_HOST env var o extrae IP de podman_tunnel.sh. _wait_for_port, mqtt_config, api_config usan SMOKE_HOST en vez de localhost hardcodeado.
4. `tests/smoke/test_smoke.py` — test_db_creation ahora limpia el db file si existe (evita IntegrityError en re-runs).

## Bugs raíz descubiertos
1. Sin .containerignore, COPY . . en el builder enviaba ~5GB vía SSH (target/, .git/) → timeout >30min
2. smoke.yaml build context para mosquitto/litestream apuntaba a repo root (../..) pero los Containerfiles usan COPY relativo al subdirectorio → fallaba con "no such file or directory"
3. Healthcheck de mosquitto usaba $SYS/broker/version (deprecado en Mosquitto 2.0, no habilitado) → mosquitto unhealthy → API bloqueada por depends_on service_healthy
4. Tests conectaban a localhost pero los contenedores corren en máquina remota (192.168.3.175) → puertos no accesibles localmente
5. Firewall del remoto bloquea puertos 30310/30311 externamente → se necesita SSH port forward

## Estado actual del stack (al momento del checkpoint)
- Stack compose: UP (servicios: mosquitto healthy, api, writer, projector, litestream)
- SSH port forwards: ACTIVOS (localhost:30310→mosquitto, localhost:30311→api)
- Tests standalone: 4/4 PASS
- Tests E2E: 12 pendientes de ejecutar

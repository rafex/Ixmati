# SPEC.md

```toml
artifact_type = "spec"
id = "SPEC-CONTAINERS-0001"
state = "active"
owner = "team-core"
created_at = "2026-07-29"
updated_at = "2026-07-29"
replaces = "none"
related_tasks = [
  "TASK-CONT-0001", "TASK-CONT-0002", "TASK-CONT-0003", "TASK-CONT-0004",
  "TASK-CONT-0005", "TASK-CONT-0006", "TASK-CONT-0007", "TASK-CONT-0008",
  "TASK-CONT-0009", "TASK-CONT-0010", "TASK-CONT-0011", "TASK-CONT-0012"
]
related_decisions = [
  "DEC-0028", "DEC-0029", "DEC-0030", "DEC-0031", "DEC-0032", "DEC-0033"
]
artifacts = ["containers/*"]
```

## Metadata

- **ID**: SPEC-CONTAINERS-0001
- **Estado**: `active`
- **Owner**: team-core

## Resumen

Establecer la infraestructura de contenedores del proyecto: Containerfiles para los 5 servicios + builder compartido, imágenes de infraestructura (Mosquitto, Litestream), compose para dev/test/prod y Quadlet para systemd rootless en el host Linux remoto.

## Problema

El proyecto no tiene definiciones de contenedor. Las referencias a `docker/` existen en 14 archivos pero la carpeta nunca se creó. El runtime de producción es Podman rootless en un host Linux amd64 accedido vía túnel SSH.

## Requisitos funcionales

- **RF-1**: `containers/base/Containerfile` compila los 5 binarios con cargo-chef.
- **RF-2**: Cada servicio tiene su `Containerfile` + symlink `Dockerfile`.
- **RF-3**: `containers/compose/dev.yaml` levanta Mosquitto para desarrollo.
- **RF-4**: `containers/compose/test.yaml` levanta Mosquitto + API en rango Temp para smoke tests.
- **RF-5**: `containers/compose/single-store.yaml` despliega el stack completo con 1 store.
- **RF-6**: `containers/compose/multi-store.yaml` despliega con 3 stores + projector.
- **RF-7**: Quadlet units para producción: network, volumen templado, mosquitto, api, `writer@` templado, projector.
- **RF-8**: `podman_tunnel.sh` gestiona el túnel SSH (up/down/status).
- **RF-9**: `podman_remote.sh` valida conexión y verifica target `amd64`.
- **RF-10**: `.containerignore` excluye `target/`, `.git/`, `docs/book/` (el contexto viaja por SSH).

## Criterios de aceptación

- **CA-1**: `podman build -f containers/base/Containerfile -t localhost/ixmati-builder:local .` completa sin error contra el remoto.
- **CA-2**: `podman build -f containers/api/Containerfile -t localhost/ixmati-api:local .` completa sin error.
- **CA-3**: `podman compose -f containers/compose/dev.yaml up -d` levanta Mosquitto y responde en `30200`.
- **CA-4**: `podman compose -f containers/compose/dev.yaml down` limpia sin residuos.
- **CA-5**: Los Quadlet units pasan validación de syntaxis de quadlet.
- **CA-6**: El registro de puertos en `containers/README.md` lista todas las asignaciones sin colisiones con puertos reservados.

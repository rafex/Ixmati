# TASKS.md

```toml
artifact_type = "task_file"
initiative = "containers"
spec_id = "SPEC-CONTAINERS-0001"
owner = "team-core"
state = "in_progress"
```
## Metadata

- **Iniciativa**: containers
- **Spec relacionada**: SPEC-CONTAINERS-0001
- **Owner**: team-core
- **Estado general**: `in_progress`

## Tareas

### TASK-CONT-0001 — .containerignore

```toml
id = "TASK-CONT-0001"
title = "Crear .containerignore y medir tamano del contexto"
state = "done"
owner = "team-core"
dependencies = []
close_criteria = ".containerignore existe y excluye target/, .git/, docs/book/, .venv/"
```

### TASK-CONT-0002 — Builder compartido

```toml
id = "TASK-CONT-0002"
title = "containers/base/Containerfile con cargo-chef"
state = "done"
owner = "team-core"
dependencies = ["TASK-CONT-0001"]
close_criteria = "Builder compila los 5 binarios en una sola pasada"
```

### TASK-CONT-0003 — Containerfiles de servicios

```toml
id = "TASK-CONT-0003"
title = "Containerfiles para api, writer, projector, supervisor, reconciler + symlinks Dockerfile"
state = "done"
owner = "team-core"
dependencies = ["TASK-CONT-0002"]
close_criteria = "5 Containerfiles + 5 symlinks Dockerfile -> Containerfile"
```

### TASK-CONT-0004 — Imagen Mosquitto

```toml
id = "TASK-CONT-0004"
title = "containers/mosquitto/ con Containerfile + mosquitto.conf (persistence, QoS 1)"
state = "done"
owner = "team-core"
dependencies = []
close_criteria = "Mosquitto configurado con persistence true, QoS 1, puertos 30200/30201"
```

### TASK-CONT-0005 — Imagen Litestream

```toml
id = "TASK-CONT-0005"
title = "containers/litestream/ con Containerfile + litestream.yml"
state = "done"
owner = "team-core"
dependencies = []
close_criteria = "Litestream sidecar con config parametrizable via env vars"
```

### TASK-CONT-0006 — Compose dev + test

```toml
id = "TASK-CONT-0006"
title = "containers/compose/dev.yaml + test.yaml"
state = "done"
owner = "team-core"
dependencies = ["TASK-CONT-0004"]
close_criteria = "dev.yaml: Mosquitto solo. test.yaml: Mosquitto + API en rango Temp"
```

### TASK-CONT-0007 — Compose single-store + multi-store

```toml
id = "TASK-CONT-0007"
title = "containers/compose/single-store.yaml + multi-store.yaml"
state = "done"
owner = "team-core"
dependencies = ["TASK-CONT-0004", "TASK-CONT-0003"]
close_criteria = "single: 1 writer. multi: 3 writers + projector. YAML anchors para DRY"
```

### TASK-CONT-0008 — Quadlet units

```toml
id = "TASK-CONT-0008"
title = "Quadlet units (network, volume@, mosquitto, api, writer@, projector)"
state = "done"
owner = "team-core"
dependencies = ["TASK-CONT-0004", "TASK-CONT-0003"]
close_criteria = "6 quadlet files, writer@.container como unit templada"
```

### TASK-CONT-0009 — Helpers y make/just

```toml
id = "TASK-CONT-0009"
title = "podman_tunnel.sh, podman_remote.sh, containers.mk, containers.just"
state = "done"
owner = "team-core"
dependencies = []
close_criteria = "Tunel up/down/status, validacion remota amd64, make/just recipes"
```

### TASK-CONT-0010 — Migrar referencias docker → podman

```toml
id = "TASK-CONT-0010"
title = "Migrar 14 archivos con referencias a docker/ y Docker"
state = "done"
owner = "team-core"
dependencies = ["TASK-CONT-0003", "TASK-CONT-0006"]
close_criteria = "0 referencias a docker/ o Docker como runtime en el repo"
```

### TASK-CONT-0011 — CI con podman

```toml
id = "TASK-CONT-0011"
title = "Actualizar CI: podman en vez de services: docker"
state = "done"
owner = "team-core"
dependencies = ["TASK-CONT-0006"]
close_criteria = "ci.yml usa podman compose en vez de services: para Mosquitto"
```

### TASK-CONT-0012 — Validación end-to-end

```toml
id = "TASK-CONT-0012"
title = "Validar build y compose dry-run contra 192.168.3.175"
state = "done"
owner = "team-core"
dependencies = ["TASK-CONT-0002", "TASK-CONT-0003", "TASK-CONT-0006"]
close_criteria = "podman build completa contra remoto; podman compose config valida syntaxis"
```

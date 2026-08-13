# Tareas — protobuf-api

### TASK-PROTO-0001 — Contrato y generación

```toml
id = "TASK-PROTO-0001"
title = "Actualizar proto y generación reproducible"
state = "done"
owner = "team-core"
close_criteria = "common/write/read compilan con tonic-prost-build y make proto valida el árbol"
validation = ["cargo check -p ixmati-api", "make proto"]
```

### TASK-PROTO-0002 — Servidor gRPC unary y autenticación

```toml
id = "TASK-PROTO-0002"
title = "Servir Write, GetWriteStatus, Read y Health por gRPC"
state = "done"
owner = "team-core"
dependencies = ["TASK-PROTO-0001"]
close_criteria = "Cliente tonic prueba los cuatro RPC, metadata x-api-key y mapeo de errores"
validation = ["cargo test -p ixmati-api", "test de integración tonic + SQLite/MQTT", "spec-native/evidence/PROTOBUF-E2E-20260812.md"]
```

### TASK-PROTO-0003 — REST/Protobuf

```toml
id = "TASK-PROTO-0003"
title = "Agregar application/protobuf y POST /read"
state = "done"
owner = "team-core"
dependencies = ["TASK-PROTO-0001"]
close_criteria = "POST /write, GET /writes, GET/POST /read y GET /health negocian Protobuf sin regresión JSON"
validation = ["cargo test -p ixmati-api", "pruebas reqwest JSON/Protobuf", "spec-native/evidence/PROTOBUF-E2E-20260812.md"]
```

### TASK-PROTO-0004 — Replay y live de eventos

```toml
id = "TASK-PROTO-0004"
title = "Stream server-side con cursor durable"
state = "in_progress"
owner = "team-core"
dependencies = ["TASK-PROTO-0001", "TASK-PROTO-0002"]
close_criteria = "Replay, transición live, filtros, OUT_OF_RANGE y backpressure tienen evidencia"
validation = ["pruebas de cursor y stream con SQLite temporal", "test local de cliente lento y RESOURCE_EXHAUSTED con cursor", "integración MQTT", "spec-native/evidence/PROTOBUF-E2E-20260812.md"]
```

### TASK-PROTO-0005 — Despliegue y documentación

```toml
id = "TASK-PROTO-0005"
title = "Publicar puertos, configuración, docs y trazabilidad"
state = "done"
owner = "team-core"
dependencies = ["TASK-PROTO-0002", "TASK-PROTO-0003", "TASK-PROTO-0004"]
close_criteria = "systemd, Compose, OpenAPI, README, runbook y SpecNative describen el contrato real"
validation = ["just validate-config", "make dist-validate", "git diff --check", "spec-native/evidence/PROTOBUF-E2E-20260812.md"]
```

### TASK-PROTO-0006 — Benchmark y cierre

```toml
id = "TASK-PROTO-0006"
title = "Comparar REST JSON, REST Protobuf y gRPC"
state = "done"
owner = "team-core"
dependencies = ["TASK-PROTO-0002", "TASK-PROTO-0003"]
close_criteria = "40/s baseline y 100/150/s diagnóstico con p50/p95/p99, errores y saturación"
validation = ["evidencia Debian amd64 desde SHA publicado", "spec-native/evidence/PROTOBUF-BENCH-20260812.md"]
```

# TASKS.md

```toml
artifact_type = "task_file"
initiative = "write-engine"
spec_id = "SPEC-WRITE-0001"
owner = "team-core"
state = "in_progress"
```

## Metadata

- **Iniciativa**: write-engine
- **Spec relacionada**: SPEC-WRITE-0001
- **Owner**: team-core
- **Estado general**: `in_progress`

## Tareas

### TASK-WRITE-0001 — Spike: viabilidad de FlashDB vía FFI en Rust

```toml
id = "TASK-WRITE-0001"
title = "Spike: viabilidad de FlashDB vía FFI en Rust"
state = "todo"
owner = "team-core"
dependencies = []
expected_files = ["spike/flashdb-ffi/Cargo.toml", "spike/flashdb-ffi/README.md"]
close_criteria = "Resultado documentado: FlashDB compila y funciona en Linux x86_64, o se descarta con justificación"
validation = ["compilación exitosa en Linux", "benchmark sintético vs sled", "evaluación de superficie unsafe"]
```

Integrar FlashDB (librería C para microcontroladores) en Rust vía `bindgen` + `cc`. Determinar si:
- Compila en Linux x86_64 como shared/static lib.
- Soporta operaciones get/set/delete con TTL en el mismo proceso.
- La superficie `unsafe` es manejable.
- El rendimiento justifica la complejidad vs alternativas puras de Rust (sled, redb, lmdb-rs).

Resultado esperado: documento de decisión (dentro o fuera) + benchmark comparativo.

### TASK-WRITE-0002 — Spike: comparativa Opción A vs Opción B

```toml
id = "TASK-WRITE-0002"
title = "Spike: comparativa Opción A (Mosquitto buffer) vs Opción B (FlashDB buffer)"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0001"]
expected_files = ["spike/comparativa/README.md", "spike/comparativa/results.json"]
close_criteria = "Datos de durabilidad, latencia y orden para ambas opciones; cierra DEC-0010 y DEC-0011"
validation = [
  "test de crash (kill -9): mensajes perdidos en cada opción",
  "medición de latencia p50/p99 de extremo a extremo",
  "test de orden con 10 productores concurrentes sobre mismo entity_id",
  "documento de recomendación con evidencia"
]
```

Implementar un prototipo mínimo de cada opción y ejecutar los experimentos definidos en `ARCHITECTURE.md` (criterios de decisión). El resultado determina la arquitectura final (cierra DEC-0010 y DEC-0011).

### TASK-WRITE-0003 — Definir contrato de envelope, topics MQTT y .proto

```toml
id = "TASK-WRITE-0003"
title = "Definir contrato de envelope, topics MQTT y archivos .proto"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0002"]
expected_files = ["proto/ixmati/v1/write.proto", "proto/ixmati/v1/read.proto", "proto/ixmati/v1/common.proto"]
close_criteria = "Archivos .proto compilan sin errores y coinciden con CONVENTIONS.md"
validation = ["protoc compila sin warnings", "revisión de pares del esquema de envelope"]
```

Definir los archivos `.proto` para gRPC y el esquema JSON del envelope según `CONVENTIONS.md`. Incluye mensajes de escritura, respuesta de ack, y consulta de estado.

### TASK-WRITE-0004 — Definir contrato de API REST (OpenAPI)

```toml
id = "TASK-WRITE-0004"
title = "Definir contrato de API REST (OpenAPI 3.0)"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0003"]
expected_files = ["api/openapi.yaml"]
close_criteria = "Especificación OpenAPI completa con todos los endpoints documentados"
validation = ["swagger-cli validate openapi.yaml", "revisión de pares"]
```

Especificación OpenAPI 3.0 para los endpoints REST: `POST /write`, `GET /writes/{idempotency_key}`, `GET /health`, `GET /read/{entity}/{key}`.

### TASK-WRITE-0005 — Implementar ixmati-core

```toml
id = "TASK-WRITE-0005"
title = "Implementar ixmati-core (tipos, envelope, errores, config)"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0003", "TASK-WRITE-0004"]
expected_files = ["crates/ixmati-core/src/lib.rs", "crates/ixmati-core/src/envelope.rs", "crates/ixmati-core/src/error.rs", "crates/ixmati-core/src/config.rs"]
close_criteria = "Tipos compilan, envelope se serializa/deserializa correctamente, errores usan thiserror, config se carga desde env/toml"
validation = ["cargo build -p ixmati-core", "tests unitarios de serialización del envelope", "tests de errores"]
```

Crate compartido sin dependencias externas pesadas. Define `WriteEnvelope`, `AckResponse`, `WriteStatus`, `WriteError`, `Config`. Es el único crate del que dependen todos los demás.

### TASK-WRITE-0006 — Implementar ixmati-writer (consumer, batching, dedup)

```toml
id = "TASK-WRITE-0006"
title = "Implementar ixmati-writer (consumer MQTT/flashdb, batching, deduplicación, BEGIN IMMEDIATE)"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0005"]
expected_files = ["crates/ixmati-writer/src/lib.rs", "crates/ixmati-writer/src/consumer.rs", "crates/ixmati-writer/src/batcher.rs", "crates/ixmati-writer/src/dedup.rs"]
close_criteria = "Writer consume mensajes, aplica batches en SQLite con deduplicación y control de versiones"
validation = [
  "test unitario de batch commit",
  "test unitario de deduplicación (mismo idempotency_key, única aplicación)",
  "test unitario de rechazo de versión obsoleta",
  "test de integración con SQLite en memoria"
]
```

Núcleo del sistema. Según el resultado del spike (DEC-0010), consume de Mosquitto o de FlashDB. Aplica escrituras en SQLite con `BEGIN IMMEDIATE`, batching acotado, validación de versiones, y registro en tabla `_idempotency`.

### TASK-WRITE-0007 — Tests de crash del writer

```toml
id = "TASK-WRITE-0007"
title = "Tests de crash del writer (kill -9, 0 pérdidas, orden preservado)"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0006"]
expected_files = ["crates/ixmati-writer/tests/crash_test.rs", "scripts/crash_test.sh"]
close_criteria = "Test de crash automatizado: publica N mensajes, kill -9, reinicia, verifica 0 pérdidas y orden"
validation = [
  "script crash_test.sh pasa con N=1000",
  "test verifica conteo exacto de mensajes en SQLite",
  "test verifica orden por entity+id"
]
```

Test de integración crítico. Un script externo (bash o Rust) lanza el writer, publica mensajes vía MQTT, envía `kill -9` al writer, verifica que Mosquitto retuvo los mensajes, reinicia el writer, y confirma que todos los mensajes están en SQLite en el orden correcto.

### TASK-WRITE-0008 — Implementar ixmati-api (REST + gRPC)

```toml
id = "TASK-WRITE-0008"
title = "Implementar ixmati-api (axum REST + tonic gRPC)"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0003", "TASK-WRITE-0004", "TASK-WRITE-0005"]
expected_files = ["crates/ixmati-api/src/lib.rs", "crates/ixmati-api/src/rest.rs", "crates/ixmati-api/src/grpc.rs"]
close_criteria = "Endpoints REST y gRPC funcionales según OpenAPI y .proto"
validation = [
  "test de integración REST con reqwest",
  "test de integración gRPC con tonic client",
  "cargo build -p ixmati-api"
]
```

Implementa los endpoints definidos en OpenAPI y `.proto`. Traduce requests HTTP/gRPC a mensajes del envelope (JSON) y los publica en el canal de ingesta.

### TASK-WRITE-0009 — Implementar modo async (ack=accepted) y sync (ack=committed)

```toml
id = "TASK-WRITE-0009"
title = "Implementar modo async y sync con correlación de respuestas"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0008", "TASK-WRITE-0006"]
expected_files = ["crates/ixmati-api/src/ack.rs", "crates/ixmati-writer/src/ack.rs"]
close_criteria = "Modo async devuelve ack inmediato; modo sync espera commit y devuelve resultado; write_id consultable"
validation = [
  "test: modo async devuelve ack en < 50ms",
  "test: modo sync devuelve ack solo tras commit en SQLite",
  "test: modo sync garantiza read-your-writes"
]
```

Implementa los dos caminos de confirmación. En modo async, el ack se genera al publicar el mensaje. En modo sync, el writer publica la confirmación en un topic de respuesta y la API espera correlacionando por `idempotency_key`.

### TASK-WRITE-0010 — Endpoint GET /writes/{idempotency_key}

```toml
id = "TASK-WRITE-0010"
title = "Implementar endpoint de consulta de estado de escritura"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0009"]
expected_files = ["crates/ixmati-api/src/status.rs"]
close_criteria = "GET /writes/{idempotency_key} devuelve estado (pending, applied, rejected) con detalles"
validation = ["test: escritura async → GET devuelve pending → luego applied", "test: idempotency_key inexistente → 404"]
```

Endpoint REST y gRPC para consultar el estado de una escritura async. El estado se obtiene de la tabla `_idempotency` en SQLite (pending = no aplicado aún, applied = commit exitoso, rejected = VERSION_CONFLICT o DUPLICATE).

### TASK-WRITE-0011 — Implementar ixmati-cache (trait CacheBackend + FlashDB o alternativa)

```toml
id = "TASK-WRITE-0011"
title = "Implementar ixmati-cache con trait CacheBackend"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0001", "TASK-WRITE-0005"]
expected_files = ["crates/ixmati-cache/src/lib.rs", "crates/ixmati-cache/src/traits.rs", "crates/ixmati-cache/src/flashdb.rs"]
close_criteria = "Trait CacheBackend definido; implementación concreta (FlashDB o alternativa) funcional con get/set/invalidate y TTL"
validation = [
  "test unitario: get/set/delete con TTL",
  "test unitario: invalidación selectiva por entity+key",
  "benchmark sintético de latencia vs SQLite directo"
]
```

Define el trait `CacheBackend` (get, set, invalidate, flush) y una implementación concreta según el resultado de DEC-0009. La cache es siempre opaca para el resto del sistema (solo se accede a través del trait).

### TASK-WRITE-0012 — Invalidación/repoblación de cache desde el writer

```toml
id = "TASK-WRITE-0012"
title = "Invalidación y repoblación de cache desde el writer tras cada commit"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0006", "TASK-WRITE-0011"]
expected_files = ["crates/ixmati-writer/src/cache_sync.rs"]
close_criteria = "Tras cada batch commit, las claves afectadas se invalidan o repueblan en cache"
validation = [
  "test: escritura → lectura inmediata desde cache devuelve valor actualizado",
  "test: invalidación por entity+key no afecta otras claves"
]
```

Extiende el writer para que, tras cada `COMMIT` exitoso, invalide las claves de cache afectadas o las repueble con los nuevos valores. Usa el trait `CacheBackend` sin depender de la implementación concreta.

### TASK-WRITE-0013 — Implementar ixmati-resync (reconstrucción offline de cache)

```toml
id = "TASK-WRITE-0013"
title = "Implementar ixmati-resync (reconstrucción de cache desde SQLite)"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0011"]
expected_files = ["crates/ixmati-resync/src/main.rs"]
close_criteria = "Binario que lee SQLite, reconstruye cache, reporta progreso y tiempo"
validation = [
  "test: resync de 100k registros en < 5s",
  "test: resync parcial por --entity",
  "test: cache resultante es idéntica a SQLite (muestreo de 1000 claves)"
]
```

Binario offline (no depende del writer corriendo). Lee SQLite en modo solo lectura, itera todas las entidades, y escribe en FlashDB. Soporta filtro por `--entity` y `--batch-size`. Reporta progreso cada 10k registros.

### TASK-WRITE-0014 — Configurar Litestream con ≥2 destinos de backup

```toml
id = "TASK-WRITE-0014"
title = "Configurar Litestream con replicación a ≥2 destinos"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0006"]
expected_files = ["config/litestream.yml", "docker/litestream.Dockerfile"]
close_criteria = "Litestream replicando WAL a S3 + VPS; comando restore verificado"
validation = [
  "litestream generations muestra generaciones en ambos destinos",
  "test de restore: borrar SQLite, ejecutar litestream restore, verificar integridad",
  "RPO medido < 5s"
]
```

Configuración de Litestream con dos réplicas. Prueba de restore automatizada. Documentación del runbook en `COMMANDS.md`.

### TASK-WRITE-0015 — Health checks integrados

```toml
id = "TASK-WRITE-0015"
title = "Implementar health checks (API, writer, Mosquitto, SQLite, cache)"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0008", "TASK-WRITE-0006", "TASK-WRITE-0011"]
expected_files = ["crates/ixmati-api/src/health.rs"]
close_criteria = "Endpoint GET /health devuelve estado de todos los componentes con detalles"
validation = [
  "health check reporta OK con todos los servicios corriendo",
  "health check reporta DEGRADED si Mosquitto no responde",
  "health check reporta DEGRADED si SQLite no responde"
]
```

Health check agregado que verifica: conectividad con Mosquitto, respuesta de SQLite (`SELECT 1`), respuesta de cache, y presencia del writer (vía heartbeat MQTT).

### TASK-WRITE-0016 — Documentar runbook de producción

```toml
id = "TASK-WRITE-0016"
title = "Documentar runbook de producción (restore, failover, resync, troubleshooting)"
state = "todo"
owner = "team-core"
dependencies = ["TASK-WRITE-0014", "TASK-WRITE-0015"]
expected_files = ["spec-native/workflows/PRODUCTION.md"]
close_criteria = "Runbook escrito con procedimientos de restore, failover manual, resync, y diagnosis"
validation = ["walkthrough manual de cada procedimiento", "revisión de pares"]
```

Documento operativo con procedimientos detallados: cómo restaurar desde Litestream, cómo hacer failover manual del writer, cómo ejecutar resync de cache, cómo diagnosticar problemas comunes (cola creciendo, SQLITE_BUSY, cache inconsistentes).

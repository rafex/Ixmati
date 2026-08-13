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
state = "in_progress"
owner = "team-core"
dependencies = []
expected_files = ["spike/flashdb-ffi/Cargo.toml", "spike/flashdb-ffi/README.md"]
close_criteria = "Resultado documentado: FlashDB compila y funciona en Linux x86_64, o se descarta con justificación"
validation = ["compilación exitosa en Linux", "benchmark sintético vs sled", "evaluación de superficie unsafe", "test de TTL e invalidación por prefijo"]
```

Integrar FlashDB (librería C para microcontroladores) en Rust vía `bindgen` + `cc`. Determinar si compila en Linux x86_64, soporta operaciones get/set/delete con TTL, y la superficie `unsafe` es manejable. **Severidad alta**: FlashDB alojará read models, no solo cache.

### TASK-WRITE-0002 — ~~Spike: comparativa Opción A vs Opción B~~ CANCELADA

```toml
id = "TASK-WRITE-0002"
title = "Spike: comparativa Opción A vs Opción B"
state = "cancelled"
owner = "team-core"
dependencies = []
```

**Motivo de cancelación**: las decisiones DEC-0010 y DEC-0011 se cerraron por diseño (no por benchmark). La Opción A (Mosquitto) es la única compatible con outbox transaccional y taxonomía de eventos separada. La Opción B (FlashDB como buffer de escritura) queda descartada. Ver DEC-0010 y DEC-0011 para justificación completa.

### TASK-WRITE-0003 — Definir contrato de envelope, topics MQTT y .proto

```toml
id = "TASK-WRITE-0003"
title = "Definir contrato de envelope (comando + evento), topics MQTT (cmd + evt) y archivos .proto"
state = "done"
owner = "team-core"
dependencies = []
expected_files = ["proto/ixmati/v1/write.proto", "proto/ixmati/v1/read.proto", "proto/ixmati/v1/common.proto"]
close_criteria = "Archivos .proto compilan sin errores y reflejan CONVENTIONS.md actualizado"
validation = ["protoc compila sin warnings", "revisión de pares del esquema de envelope (comando y evento)", "taxonomía cmd/evt validada"]
```

Definir los archivos `.proto` para gRPC y el esquema JSON del envelope de comando y de evento según `CONVENTIONS.md`. Incluye topics `ixmati/cmd/...` y `ixmati/evt/...` con semántica separada. El campo `store` es obligatorio en comandos.

### TASK-WRITE-0004 — Definir contrato de API REST (OpenAPI)

```toml
id = "TASK-WRITE-0004"
title = "Definir contrato de API REST (OpenAPI 3.0)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0003"]
expected_files = ["api/openapi.yaml"]
close_criteria = "Especificación OpenAPI completa con todos los endpoints documentados"
validation = ["swagger-cli validate openapi.yaml", "revisión de pares"]
```

Especificación OpenAPI 3.0: `POST /write`, `GET /writes/{store}/{idempotency_key}`, `GET /read` (por store o por proyección), `GET /health`. El campo `store` es obligatorio en escritura.

### TASK-WRITE-0005 — Implementar ixmati-core

```toml
id = "TASK-WRITE-0005"
title = "Implementar ixmati-core (tipos, envelope comando + evento, errores, config, StoreConfig)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0003", "TASK-WRITE-0004"]
expected_files = ["crates/ixmati-core/src/lib.rs", "crates/ixmati-core/src/envelope.rs", "crates/ixmati-core/src/error.rs", "crates/ixmati-core/src/config.rs", "crates/ixmati-core/src/store.rs"]
close_criteria = "Tipos compilan, envelopes se serializan correctamente, StoreConfig se carga desde env/toml"
validation = ["cargo build -p ixmati-core", "tests de serialización del envelope de comando y evento", "tests de carga de config multi-store"]
```

Crate compartido. Define `WriteEnvelope`, `EventEnvelope`, `AckResponse`, `WriteStatus`, `Error`, `Config`, `StoreConfig`. Incluye lógica de proyección compartida (para projector y reconciler). El campo `store` es obligatorio en `WriteEnvelope`.

### TASK-WRITE-0006 — Implementar ixmati-writer (consumer, batching, dedup, outbox)

```toml
id = "TASK-WRITE-0006"
title = "Implementar ixmati-writer (consumer MQTT, batching, deduplicación, BEGIN IMMEDIATE, outbox transaccional)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0005"]
expected_files = ["crates/ixmati-writer/src/lib.rs", "crates/ixmati-writer/src/consumer.rs", "crates/ixmati-writer/src/batcher.rs", "crates/ixmati-writer/src/dedup.rs", "crates/ixmati-writer/src/outbox.rs"]
close_criteria = "Writer consume comandos, aplica batches con outbox transaccional (datos + _idempotency + _outbox en misma tx)"
validation = [
  "test unitario de batch commit con outbox",
  "test unitario de deduplicación por (store, idempotency_key)",
  "test unitario de rechazo de versión obsoleta",
  "test unitario: _outbox contiene el evento tras commit",
  "test de integración con SQLite en memoria"
]
```

Núcleo del sistema. Consume de `ixmati/cmd/<store>/...`. Cada writer maneja exactamente un store. Aplica batches con `BEGIN IMMEDIATE`. La transacción incluye: datos de la entidad, `_idempotency`, y `_outbox`. El publicador de eventos es una task interna del mismo proceso.

### TASK-WRITE-0007 — Tests de crash del writer

```toml
id = "TASK-WRITE-0007"
title = "Tests de crash del writer (kill -9, 0 comandos perdidos, 0 eventos perdidos, orden preservado)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0006", "TASK-WRITE-0018"]
expected_files = ["helpers/shell/kill9_writer.sh", "tests/smoke/test_crash_durability.py"]
close_criteria = "Test de crash automatizado: publica N comandos, kill -9, reinicia, verifica 0 comandos perdidos, 0 eventos perdidos, orden preservado"
validation = [
  "crash_test.sh pasa con N=1000 y 1 store",
  "crash_test.sh pasa con N=1000 y 3 stores (aislamiento)",
  "test verifica que _outbox se vacía tras reinicio (todos los eventos publicados)"
]
```

Test de integración crítico. Un script externo lanza N writers (uno por store), publica comandos vía MQTT, `kill -9` un writer, verifica que Mosquitto retuvo los comandos, reinicia, y confirma que todos los comandos están en SQLite y todos los eventos del outbox fueron publicados.

### TASK-WRITE-0008 — Implementar ixmati-api (REST + gRPC)

```toml
id = "TASK-WRITE-0008"
title = "Implementar ixmati-api (axum REST + tonic gRPC)"
state = "in_progress"
owner = "team-core"
dependencies = ["TASK-WRITE-0003", "TASK-WRITE-0004", "TASK-WRITE-0005"]
expected_files = ["crates/ixmati-api/src/lib.rs", "crates/ixmati-api/src/rest.rs", "crates/ixmati-api/src/grpc.rs"]
close_criteria = "Endpoints REST y gRPC funcionales según OpenAPI y .proto"
validation = ["test de integración REST con reqwest", "test de integración gRPC con tonic client", "cargo build -p ixmati-api"]
```

Implementa los endpoints definidos. Traduce requests HTTP/gRPC a comandos JSON y los publica en `ixmati/cmd/<store>/...`. Sirve lecturas desde FlashDB con fallback a SQLite (cache-aside y proyecciones). La implementación gRPC y REST/Protobuf está en progreso; sólo volverá a `done` cuando existan pruebas de integración reales con cliente tonic y reqwest.

### TASK-WRITE-0009 — Implementar modo async y sync con correlación

```toml
id = "TASK-WRITE-0009"
title = "Implementar modo async/sync con correlación de respuestas"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0008", "TASK-WRITE-0006"]
expected_files = ["crates/ixmati-api/src/ack.rs", "crates/ixmati-writer/src/ack.rs"]
close_criteria = "accepted y committed esperan commit durable; 200 devuelve resultado aplicado, 202 deja estado consultable por (store, idempotency_key)"
validation = [
  "test: modo async devuelve ack en < 50ms",
  "test: modo sync devuelve ack solo tras commit en SQLite del store correcto",
  "test: modo sync garantiza read-your-writes en el store del comando",
  "test: comando sin store → rechazado con error"
]
```

Dos caminos de confirmación. En modo async, el ack se genera al publicar en Mosquitto. En modo sync, el writer publica la confirmación en un topic de respuesta y la API espera correlacionando por `(store, idempotency_key)`.

### TASK-WRITE-0010 — Endpoint GET /writes/{store}/{idempotency_key}

```toml
id = "TASK-WRITE-0010"
title = "Implementar endpoint de consulta de estado de comando"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0009"]
expected_files = ["crates/ixmati-api/src/status.rs"]
close_criteria = "GET /writes/{store}/{idempotency_key} devuelve estado (pending, applied, rejected) con detalles"
validation = ["test: comando async → GET devuelve pending → luego applied", "test: idempotency_key inexistente → 404", "test: store inexistente → 404"]
```

Endpoint REST y gRPC para consultar el estado de un comando async. El estado se obtiene de `_idempotency` en el SQLite del store correspondiente.

### TASK-WRITE-0011 — Implementar ixmati-cache (trait CacheBackend + FlashDB o alternativa)

```toml
id = "TASK-WRITE-0011"
title = "Implementar ixmati-cache (modulo, trait CacheBackend, NoOp para single-store)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0001", "TASK-WRITE-0005"]
expected_files = ["crates/ixmati-cache/src/lib.rs", "crates/ixmati-cache/src/traits.rs", "crates/ixmati-cache/src/flashdb.rs"]
close_criteria = "Trait CacheBackend definido con namespaces c: y p:; implementación concreta funcional con get/set/invalidate/delete_by_prefix y TTL"
validation = [
  "test: get/set/delete con TTL en namespace c:",
  "test: invalidación por prefijo (c:store:*)",
  "test: namespaces aislados (purga de c: no afecta p:)",
  "benchmark sintético de latencia vs SQLite directo"
]
```

Define `CacheBackend` con namespaces `c:` (cache-aside) y `p:` (proyecciones). La purga de uno no afecta al otro. Keyspace unificado con prefijo `<store>:<entity>:<key>`.

### TASK-WRITE-0012 — Invalidación/repoblación de cache-aside desde el writer

```toml
id = "TASK-WRITE-0012"
title = "Invalidación y repoblación de cache-aside desde el writer tras cada commit"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0006", "TASK-WRITE-0011"]
expected_files = ["crates/ixmati-writer/src/cache_sync.rs"]
close_criteria = "Tras cada batch commit, las claves c: afectadas se invalidan o repueblan"
validation = [
  "test: comando → lectura desde c: devuelve valor actualizado",
  "test: invalidación de c:store:* no afecta p:* ni otros stores"
]
```

Extiende el writer para que, tras cada `COMMIT`, invalide o repueble las claves `c:<store>:<entity>:<key>` afectadas. Usa `CacheBackend`. No afecta proyecciones (`p:*`) ni otros stores.

### TASK-WRITE-0013 — ~~Implementar ixmati-resync~~ → reemplazada por TASK-WRITE-0022

```toml
id = "TASK-WRITE-0013"
title = "Implementar ixmati-resync (reconstrucción de cache desde SQLite)"
state = "cancelled"
owner = "team-core"
dependencies = []
```

**Motivo**: reemplazada por `TASK-WRITE-0022` (reconciler fan-in). La cache-aside no se reconstruye offline (se repuebla con el tráfico). Los read models se reproyectan vía reconciler. Ver DEC-0019 y DEC-0020.

### TASK-WRITE-0014 — Configurar Litestream por store con ≥2 destinos

```toml
id = "TASK-WRITE-0014"
title = "Configurar Litestream con replicación a ≥2 destinos por store"
state = "in_progress"
owner = "team-core"
dependencies = ["TASK-WRITE-0006", "TASK-WRITE-0017"]
expected_files = ["config/litestream.yml", "docker/litestream.Dockerfile"]
close_criteria = "Litestream replicando WAL de cada store a S3 + VPS; restore verificado por store y RPO/RTO medidos"
validation = [
  "litestream generations muestra generaciones en ambos destinos por store",
  "test de restore: borrar SQLite de un store, litestream restore, verificar integridad",
  "RPO medido < 5s para store crítico"
]
```

La imagen sidecar y la configuración declarativa existen, pero la tarea vuelve a
`in_progress` hasta demostrar el flujo completo en el despliegue objetivo:
replicación a dos destinos, restore destructivo verificado y RPO/RTO medidos.
La instalación nativa actual no instala ni arranca Litestream automáticamente.

### TASK-WRITE-0015 — Health checks integrados

```toml
id = "TASK-WRITE-0015"
title = "Implementar health checks (API, writer por store, Mosquitto, SQLite por store, cache)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0008", "TASK-WRITE-0006", "TASK-WRITE-0011"]
expected_files = ["crates/ixmati-api/src/health.rs"]
close_criteria = "GET /health devuelve estado de todos los componentes y stores con detalles"
validation = [
  "health check reporta OK con todos los servicios y stores",
  "health check reporta DEGRADED si un writer no responde (otros stores OK)",
  "health check reporta store específico como unhealthy sin afectar el estado global"
]
```

Health check agregado por store. Verifica: conectividad Mosquitto, respuesta SQLite (`SELECT 1`) por store, respuesta cache, presencia de cada writer (vía heartbeat MQTT por store).

### TASK-WRITE-0016 — Documentar runbook de producción

```toml
id = "TASK-WRITE-0016"
title = "Documentar runbook de producción (restore por store, failover, reproyección, troubleshooting)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0014", "TASK-WRITE-0015", "TASK-WRITE-0022"]
expected_files = ["docs/src/operations/runbook.md"]
close_criteria = "Runbook escrito con procedimientos de restore por store, failover manual, reproyección, y diagnóstico"
validation = ["walkthrough manual de cada procedimiento", "revisión de pares"]
```

Documento operativo: cómo restaurar un store desde Litestream, failover manual del writer de un store, reproyección completa/selectiva con reconciler, diagnóstico (lag de cola, lag de proyección, store caliente, outbox creciendo).

### TASK-WRITE-0017 — Store registry + config multi-store

```toml
id = "TASK-WRITE-0017"
title = "Implementar store registry y configuración multi-store"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0005"]
expected_files = ["crates/ixmati-core/src/store.rs", "config/stores.example.toml"]
close_criteria = "Config de stores cargable desde TOML/env; stores=1 funciona sin overhead; stores=N activa componentes opcionales"
validation = [
  "test: config con 1 store → no activa bus de eventos ni outbox",
  "test: config con 3 stores → writer por store, topics independientes",
  "test: store renombrado → error de validación (no migration path)"
]
```

Define `StoreConfig` y el registry. Resuelve topología: `N` pods vs 1 proceso con `N` writer tasks, configurable. Con `stores=1`, el sistema no activa bus de eventos, outbox ni proyectores. Stores son inmutables tras creación (renombrar = migración).

### TASK-WRITE-0018 — Tabla _outbox + publicador transaccional

```toml
id = "TASK-WRITE-0018"
title = "Implementar tabla _outbox, inserción transaccional, y publicador de eventos como task del writer"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0006"]
expected_files = ["crates/ixmati-writer/src/outbox.rs", "crates/ixmati-writer/src/publisher.rs"]
close_criteria = "Evento insertado en _outbox en misma tx que datos; publicador lee y publica con at-least-once; mark publicado idempotente"
validation = [
  "test: commit exitoso → fila en _outbox con published_at NULL",
  "test: publicador publica y marca published_at",
  "test: crash entre commit y publish → tras reinicio, evento se publica (0 pérdidas)",
  "test: limpieza periódica de outbox (filas antiguas)"
]
```

Tabla `_outbox(id, event_type, event_id, store, entity, key, version, occurred_at, payload, published_at)`. Insertada en la misma `BEGIN IMMEDIATE`. Publicador interno sondea o usa `sqlite3_update_hook`. Publica en `ixmati/evt/<store>/<entity>/<id>` con QoS 1 y marca `published_at`. Limpieza periódica de filas con > 7 días de antigüedad.

### TASK-WRITE-0019 — EventEnvelope + bus de eventos

```toml
id = "TASK-WRITE-0019"
title = "Definir EventEnvelope y publicación en ixmati/evt/..."
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0003", "TASK-WRITE-0018"]
expected_files = ["crates/ixmati-core/src/event.rs", "crates/ixmati-writer/src/publisher.rs"]
close_criteria = "EventEnvelope definido; publicador emite en ixmati/evt/<store>/<entity>/<id> con formato correcto"
validation = [
  "test: evento publicado contiene store, event_id, event_type, version, occurred_at, payload",
  "test: topic evt separado de cmd",
  "test: versión de evento coincide con versión del comando que lo generó"
]
```

Define `EventEnvelope` con `store`, `entity`, `key`, `version`, `event_type`, `occurred_at`, `event_id` (UUID), `payload`. Publicado en `ixmati/evt/<store>/<entity>/<id>`. Separación estricta de topics cmd/evt.

### TASK-WRITE-0020 — ixmati-projector: runtime de proyecciones idempotentes

```toml
id = "TASK-WRITE-0020"
title = "Implementar ixmati-projector (runtime de proyecciones, idempotencia, patrón R y M)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0011", "TASK-WRITE-0019"]
expected_files = ["crates/ixmati-projector/src/lib.rs", "crates/ixmati-projector/src/projection.rs", "crates/ixmati-projector/src/idempotency.rs"]
close_criteria = "Projector consume eventos de ixmati/evt/... y actualiza read models p:<projection>:<key> de forma idempotente"
validation = [
  "test: evento → proyección actualizada",
  "test: mismo event_id 3 veces → proyección actualizada 1 vez",
  "test: patrón R (lookup) y M (materializado) según config",
  "test: fan-out de patrón M validado contra DEC-0016"
]
```

Consume eventos de `ixmati/evt/...`, aplica proyecciones declaradas en config. Idempotente por `event_id` (trackea IDs procesados) o por upsert natural (si la proyección es inherentemente idempotente). Soporta patrón R y M. Valida regla de fan-out (DEC-0016) al cargar proyecciones.

### TASK-WRITE-0021 — Declaración de proyecciones en config

```toml
id = "TASK-WRITE-0021"
title = "Declaración de proyecciones en config (patrón R/M, stores fuente, campos, clave destino)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0020"]
expected_files = ["config/projections.example.toml", "crates/ixmati-core/src/projection_config.rs"]
close_criteria = "Proyecciones declarables en TOML; validadas al cargar (fan-out ≤ 100 para M, stores fuente existen, clave destino única)"
validation = [
  "test: proyección R válida se carga sin error",
  "test: proyección M con fan_out > 100 → rechazada en validación",
  "test: proyección referencia store inexistente → error de validación"
]
```

Formato de declaración: nombre, stores fuente, patrón (R/M), campos copiados (M), clave destino en FlashDB (`p:<name>:<key>`). Validación al cargar. Sin proyecciones declaradas, el projector no hace nada.

### TASK-WRITE-0022 — ixmati-reconciler: reproyección fan-in sobre N stores

```toml
id = "TASK-WRITE-0022"
title = "Implementar ixmati-reconciler (reproyección offline fan-in sobre N stores)"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0011", "TASK-WRITE-0021"]
expected_files = ["crates/ixmati-reconciler/src/main.rs"]
close_criteria = "Binario que lee N stores en modo solo lectura, reproyecta read models p:*, reporta progreso"
validation = [
  "test: reproyección total con 3 stores y 100k registros combinados",
  "test: reproyección selectiva --projection <name>",
  "test: read models resultantes idénticos a proyección online (muestreo)",
  "test: --dry-run reporta sin escribir"
]
```

Binario offline. Modo fan-in: para cada proyección declarada, lee los stores fuente, aplica la lógica de proyección (compartida con `ixmati-core`), y escribe en FlashDB (`p:*`). Soporta `--projection`, `--dry-run`. No toca `c:*` (eso es cache-aside, se repuebla con tráfico).

### TASK-WRITE-0023 — ATTACH DATABASE read-only para analítica

```toml
id = "TASK-WRITE-0023"
title = "Habilitar ATTACH DATABASE en conexiones read-only para reporting cross-store"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0006", "TASK-WRITE-0017"]
expected_files = ["crates/ixmati-core/src/attach.rs"]
close_criteria = "Conexiones read-only pueden usar ATTACH; conexiones write lo tienen bloqueado"
validation = [
  "test: ATTACH en conexión write → error",
  "test: ATTACH en conexión read-only → consulta cross-store exitosa",
  "test: stores en ATTACH no interfieren con writer (sin locks adicionales)"
]
```

Habilita `ATTACH DATABASE` exclusivamente en conexiones de solo lectura, sobre réplicas co-localizadas. Útil para reporting ad-hoc. Bloqueado a nivel de PRAGMA en conexiones write.

### TASK-WRITE-0024 — ixmati-supervisor + K8s manifests por store

```toml
id = "TASK-WRITE-0024"
title = "Implementar ixmati-supervisor y manifiestos Kubernetes por store"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0017", "TASK-WRITE-0014"]
expected_files = ["crates/ixmati-supervisor/src/main.rs", "k8s/deployment.yaml", "k8s/pvc.yaml", "k8s/litestream-sidecar.yaml"]
close_criteria = "Supervisor orquesta stores según topología; K8s manifiestos despliegan 1 pod por store con PVC y Litestream sidecar"
validation = [
  "test: supervisor con stores=1 lanza 1 writer task",
  "test: supervisor con stores=3 lanza 3 writer tasks independientes",
  "test: K8s manifests aplican sin errores (dry-run)"
]
```

Supervisor orquesta múltiples stores. Topología configurable: `N` pods (K8s, 1 pod por store) o 1 proceso con `N` writer tasks (single-VPS). Manifiestos K8s: Deployment/StatefulSet por store, PVC por store, Litestream sidecar por store.

### TASK-WRITE-0025 — Tests de consistencia eventual

```toml
id = "TASK-WRITE-0025"
title = "Tests de consistencia eventual: outbox, lag de proyección, reproyección"
state = "done"
owner = "team-core"
dependencies = ["TASK-WRITE-0018", "TASK-WRITE-0020", "TASK-WRITE-0022"]
expected_files = ["tests/smoke/test_outbox.py", "tests/smoke/test_projection_lag.py", "tests/smoke/test_crash_durability.py"]
close_criteria = "Suite de tests que validan 0 eventos perdidos, idempotencia de proyecciones, y reproyección correcta"
validation = [
  "test: crash entre commit y publish → 0 eventos perdidos (outbox)",
  "test: re-entrega de evento → proyección idempotente",
  "test: lag de proyección < 500ms p99 bajo carga normal (100 evt/s)",
  "test: reproyección completa → idéntico a proyección online",
  "test: caída de store A → proyecciones que referencian A siguen sirviendo"
]
```

Suite de tests de integración para consistencia eventual. Valida las garantías del outbox, la idempotencia de proyectores, el lag en condiciones normales, y la integridad de la reproyección.

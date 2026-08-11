# SPEC.md

```toml
artifact_type = "spec"
id = "SPEC-WRITE-0001"
state = "done"
owner = "team-core"
created_at = "2026-07-29"
P26-07-30"
replaces = "none"
related_tasks = [
  "TASK-WRITE-0001", "TASK-WRITE-0003", "TASK-WRITE-0004",
  "TASK-WRITE-0005", "TASK-WRITE-0006", "TASK-WRITE-0007", "TASK-WRITE-0008",
  "TASK-WRITE-0009", "TASK-WRITE-0010", "TASK-WRITE-0011", "TASK-WRITE-0012",
  "TASK-WRITE-0013", "TASK-WRITE-0014", "TASK-WRITE-0015", "TASK-WRITE-0016",
  "TASK-WRITE-0017", "TASK-WRITE-0018", "TASK-WRITE-0019", "TASK-WRITE-0020",
  "TASK-WRITE-0021", "TASK-WRITE-0022", "TASK-WRITE-0023", "TASK-WRITE-0024",
  "TASK-WRITE-0025"
]
related_decisions = [
  "DEC-0001", "DEC-0002", "DEC-0003", "DEC-0004", "DEC-0005",
  "DEC-0007", "DEC-0008", "DEC-0009", "DEC-0010", "DEC-0011",
  "DEC-0012", "DEC-0013", "DEC-0014", "DEC-0015", "DEC-0016",
  "DEC-0017", "DEC-0018", "DEC-0019", "DEC-0020"
]
artifacts = ["crates/*", "proto/*", "docker/*", "config/*"]
validation = [
  "spike FlashDB FFI",
  "test de crash del writer (kill -9, 0 mensajes perdidos, 0 eventos perdidos)",
  "test de orden bajo concurrencia (10 writers, mismo entity_id)",
  "test de idempotencia (QoS 1 reintentos, proyecciones idempotentes)",
  "benchmark de throughput (escrituras/segundo, lecturas/segundo)",
  "test de proyección (re-entrega de evento, idempotencia)",
  "test de outbox (crash entre commit y publish → 0 eventos perdidos)",
  "test de reproyección completa (fan-in sobre N stores)",
  "test de restore Litestream por store (RPO < 5s, RTO < 60s)"
]
```

## Metadata

- **ID**: SPEC-WRITE-0001
- **Estado**: `active`
- **Owner**: team-core
- **Fecha de creación**: 2026-07-29
- **Última actualización**: 2026-07-29
- **Reemplaza**: `none`
- **Tareas relacionadas**: `TASK-WRITE-0001` a `TASK-WRITE-0025` (excluye `TASK-WRITE-0002`, cancelada)
- **Decisiones relacionadas**: `DEC-0001` a `DEC-0020` (excluye `DEC-0006`, superseded)

## Resumen

Crear un motor en Rust que permita a múltiples backends/pods escribir en SQLite sin contención. La unidad atómica es el **store** (archivo SQLite + writer + topic + backup). N stores = N escritores independientes. Comandos vía Mosquitto (persistente, QoS 1), escritura serializada con outbox transaccional, bus de eventos para proyecciones, cache-aside + read models en FlashDB, backup continuo con Litestream por store.

## Problema

Múltiples backends necesitan escribir en SQLite, pero SQLite solo soporta un escritor simultáneo por archivo. Abrir conexiones de escritura desde cada backend provoca `SQLITE_BUSY`. No se quiere introducir un RDBMS distribuido. Además, en arquitecturas con múltiples bounded contexts, se necesita aislamiento de fallo y evolución desacoplada de esquemas.

## Objetivo

Los backends envían comandos de escritura sin preocuparse por concurrencia, bloqueos, o pérdida de datos. El sistema garantiza orden por store+entity+id, idempotencia, y ofrece dos modos de confirmación (async/sync). Las lecturas se sirven desde cache-aside (por defecto) o desde read models proyectados (opt-in) vía bus de eventos. La recuperación ante desastres está automatizada con Litestream por store. El motor funciona con 1 store (sin overhead de eventos) y escala a N sin cambiar de paradigma.

## Alcance

**Incluye**:
- Abstracción de Store como primitivo configurable (`stores=1` es el caso base sin overhead).
- API REST + gRPC para comando, consulta de estado y lectura.
- Mosquitto como canal de ingesta de comandos, con persistence y QoS 1.
- Writer único por store con batching, deduplicación y `BEGIN IMMEDIATE`.
- Transactional outbox: el evento se inserta en `_outbox` en la misma transacción que el dato.
- Publicador de eventos como task interna del writer (at-least-once, sin dual-write).
- Bus de eventos separado (`ixmati/evt/...`) de los comandos (`ixmati/cmd/...`).
- Proyectores idempotentes que actualizan read models en FlashDB desde eventos.
- Proyecciones opt-in declaradas en config (patrón R y M con regla de decisión explícita).
- Cache-aside (`c:*`) coexistente con proyecciones (`p:*`) en FlashDB.
- `ack=accepted` se mantiene como alias compatible de `ack=committed`; ambos
  esperan confirmación durable. Un `200` implica commit SQLite y un timeout
  produce `202 PENDING`.
- Endpoint `GET /writes/{store}/{idempotency_key}`.
- Reconciler offline: reproyección fan-in sobre N stores.
- Litestream por store hacia ≥2 destinos con RPO < 5s y RTO < 60s.
- Health checks integrados.
- Métricas (lag de cola, lag de proyección, latencia de commit, tasa de misses).

**Excluye**:
- Replicación multi-master o escritura concurrente directa a SQLite.
- Sharding horizontal dentro de un store (particionado de carga, no de dominio).
- Transacciones cross-store; sagas y compensación (responsabilidad de la aplicación).
- Orquestador de sagas en el motor.
- JOIN SQL cross-store operacional (ATTACH read-only para analítica sí está permitido).
- Failover automático del writer (manual u orquestado externamente).
- CDC para suscriptores externos (backlog).

## Requisitos funcionales

- **RF-1**: la API acepta comandos de escritura vía REST (JSON) y gRPC (protobuf) con el envelope definido en `CONVENTIONS.md`. El campo `store` es obligatorio.
- **RF-2**: el writer consume comandos de `ixmati/cmd/<store>/<entity>/<id>` y aplica escrituras en el SQLite del store de forma serializada.
- **RF-3**: el writer soporta operaciones `upsert`, `delete`, y `patch`.
- **RF-4**: el writer rechaza comandos duplicados (`idempotency_key` ya procesada en ese store) con `DUPLICATE`.
- **RF-5**: el writer rechaza comandos con versión obsoleta (`version <= stored_version`) con `VERSION_CONFLICT`.
- **RF-6**: el modo `ack=accepted` devuelve `200` sólo después de que el
  writer confirma el commit SQLite; si aún no puede confirmarlo devuelve
  `202 PENDING` con estado consultable.
- **RF-7**: el modo `ack=committed` devuelve ack solo tras commit exitoso en SQLite del store correspondiente.
- **RF-8**: existe un endpoint `GET /writes/{store}/{idempotency_key}` que devuelve el estado de un comando async.
- **RF-9**: las lecturas por `store/entity/key` se sirven desde cache-aside (`c:<store>:<entity>:<key>` en FlashDB). Si hay miss, se consulta SQLite del store y se repuebla la cache.
- **RF-10**: el writer invalida o repuebla la cache-aside tras cada commit exitoso.
- **RF-11**: el writer inserta un evento en `_outbox` dentro de la misma transacción que los datos (outbox transaccional).
- **RF-12**: el publicador (task interna del writer) lee `_outbox`, publica en `ixmati/evt/<store>/<entity>/<id>`, y marca `published_at`.
- **RF-13**: los proyectores consumen eventos de `ixmati/evt/...` y actualizan read models en FlashDB (`p:<projection>:<key>`) de forma idempotente.
- **RF-14**: las proyecciones se declaran en config y soportan patrón R (lookup) y M (materializado).
- **RF-15**: el reconciler reconstruye read models desde los stores fuente (fan-in sobre N stores). Soporta reproyección total y selectiva (`--projection`).
- **RF-16**: Litestream replica el WAL de cada store a destinos configurados de forma continua.
- **RF-17**: el sistema funciona con `stores=1` sin bus de eventos, outbox ni proyectores activos (caso base).
- **RF-18**: `ATTACH DATABASE` está permitido exclusivamente en conexiones read-only para reporting analítico.

## Requisitos no funcionales

- **RNF-1**: 0 comandos perdidos ante `kill -9` del writer y reinicio (Mosquitto persistence + QoS 1).
- **RNF-2**: 0 eventos perdidos ante crash del writer entre commit y publicación (outbox transaccional).
- **RNF-3**: 0 errores `SQLITE_BUSY` propagados al cliente en condiciones normales.
- **RNF-4**: p99 de latencia de ack en modo async < 50ms.
- **RNF-5**: orden de comandos preservado por `(store, entity, id)`.
- **RNF-6**: read-your-writes garantizado en modo `committed`, acotado al store del comando.
- **RNF-7**: RPO < 5 segundos por store, RTO < 60 segundos.
- **RNF-8**: la API soporta ≥ 1000 comandos/segundo sostenidos sin degradación (distribuidos entre stores).
- **RNF-9**: la API soporta ≥ 500 lecturas/segundo sostenidas desde cache sin degradación.
- **RNF-10**: el reconciler procesa ≥ 50k registros/segundo en reproyección.
- **RNF-11**: lag de proyección p99 < 500ms en condiciones normales.
- **RNF-12**: todos los comandos y eventos se loguean con `store`, `idempotency_key`/`event_id`, `entity`, `key`, `op`, `version`, `latency_ms`.
- **RNF-13**: una caída de un store no afecta las lecturas de read models que referencian datos de ese store (los datos proyectados están en FlashDB).
- **RNF-14**: la escritura en store A no genera contención observable en store B.

## Criterios de aceptación

- **CA-1 — Escritura concurrente sin bloqueos**: 10 backends escribiendo sobre stores distintos, 0 `SQLITE_BUSY`.
- **CA-2 — Crash del writer sin pérdida**: `kill -9` del writer de un store, reinicio, 0 comandos perdidos.
- **CA-3 — Orden preservado**: 2 backends escribiendo sobre el mismo `(store, entity, id)` con versiones 1,2,3 → versión final correcta sin regresiones.
- **CA-4 — Idempotencia de comandos**: mismo `idempotency_key` 3 veces → 1 aplicación, 2 `DUPLICATE`.
- **CA-5 — Rechazo de versión obsoleta**: `version=3` sobre registro con `version=5` → `VERSION_CONFLICT`.
- **CA-6 — Compatibilidad de ack**: comando `ack_mode=accepted` se acepta como
  alias durable; `200` sólo se entrega después del commit SQLite. Si vence la
  espera, responde `202 PENDING` y el estado sigue siendo consultable.
- **CA-7 — Modo sync con read-your-writes`: comando `ack_mode=committed` exitoso → lectura inmediata devuelve datos actualizados.
- **CA-8 — Cache miss con fallback**: cache sin el registro → lectura de SQLite → cache repoblada → dato correcto.
- **CA-9 — Invalidación de cache tras escritura**: comando aplicado → siguiente lectura por cache devuelve valor actualizado.
- **CA-10 — Outbox transaccional**: `kill -9` entre commit y publish → tras reinicio, 0 eventos perdidos (todos los del outbox se publican).
- **CA-11 — Proyección idempotente**: mismo `event_id` 3 veces → read model actualizado una sola vez.
- **CA-12 — Read model sobrevive a caída de store fuente**: store A cae, read model que contiene datos de A en FlashDB sigue sirviendo lecturas.
- **CA-13 — Reproyección completa**: N stores con datos, `ixmati-reconciler` → read models reconstruidos idénticos a una proyección online.
- **CA-14 — Aislamiento entre stores**: escritura intensiva en store A → 0 `SQLITE_BUSY` en store B.
- **CA-15 — Caso base stores=1**: despliegue con un solo store → sin bus de eventos, sin outbox activo, sin proyectores. El sistema funciona igual que el diseño original.
- **CA-16 — Restore de backup**: `litestream restore` por store → RPO < 5s, integridad verificada, servicio en < 60s.

## Dependencias y riesgos

- **Dependencia**: Mosquitto como broker MQTT. Requiere persistence y QoS 1.
- **Dependencia**: FlashDB vía FFI (riesgo DEC-0009). Si no es viable, migrar a sled/redb/lmdb-rs.
- **Dependencia**: Litestream para replicación WAL por store. Requiere acceso a S3 o filesystem remoto.
- **Riesgo**: el writer de un store es single-node; si cae, los comandos de ese store esperan (Mosquitto retiene).
- **Riesgo**: FlashDB en servidor Linux puede no ofrecer las mismas propiedades que en embebidos. El spike lo determina.
- **Riesgo**: store caliente (un store recibe el 90% del tráfico). El sharding por store no resuelve esto; requiere partición interna o migración de motor.

## Plan de ejecución

Ver `TASKS.md` para el desglose completo y `ROADMAP.md` para las fases.

1. **Fase 0 — Contratos y decisión técnica**:
   - Spike FlashDB FFI (`0001`).
   - Contratos de envelope, topics (cmd + evt), .proto, OpenAPI (`0003`, `0004`).
2. **Fase 1 — Core + store + writer**:
   - `ixmati-core` con tipos, StoreConfig, envelope de comando y evento (`0005`).
   - Store registry y multi-store config (`0017`).
   - Writer por store con consumer, batching, dedup, outbox transaccional (`0006`, `0018`).
   - Tests de crash (`0007`).
3. **Fase 2 — API**:
   - REST + gRPC (`0008`).
   - Modos async/sync (`0009`).
   - Endpoint de estado (`0010`).
4. **Fase 3 — Cache y proyecciones**:
   - Cache-aside con `CacheBackend` (`0011`, `0012`).
   - Bus de eventos + `EventEnvelope` (`0019`).
   - Projector runtime + declaración de proyecciones (`0020`, `0021`).
   - Reconciler fan-in (`0022`).
5. **Fase 4 — DR y producción**:
   - Litestream por store (`0014`).
   - ATTACH read-only para analítica (`0023`).
   - Health checks (`0015`).
   - Supervisor + K8s manifests (`0024`).
   - Runbook (`0016`).
6. **Fase 5 — Observabilidad y calidad**:
   - Tests de consistencia eventual (`0025`).
   - Métricas, backpressure, alertas, benchmarks.

## Plan de validación

- Spike FlashDB FFI (`TASK-WRITE-0001`): compilar + benchmark sintético vs sled.
- Test de crash del writer (`TASK-WRITE-0007`): N comandos, `kill -9`, 0 pérdidas, orden preservado.
- Test de outbox (`TASK-WRITE-0025`): crash entre commit y publish, 0 eventos perdidos.
- Test de proyección idempotente: mismo `event_id` 3 veces, 1 sola aplicación.
- Test de aislamiento entre stores: carga en store A, 0 impacto en store B.
- Test de reproyección completa: fan-in sobre N stores, idéntico a proyección online.
- Test de caso base `stores=1`: sin eventos, sin outbox, sin proyectores. Mismas garantías que el diseño original.
- Benchmark de throughput multi-store.
- Test de restore Litestream por store.

## Trazabilidad

- **Commits o PRs**: pendiente
- **Archivos principales**: `crates/*`, `proto/*`, `docker/*`, `config/*`
- **Resultado de validación**: pendiente

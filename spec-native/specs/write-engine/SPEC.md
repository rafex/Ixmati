# SPEC.md

```toml
artifact_type = "spec"
id = "SPEC-WRITE-0001"
state = "active"
owner = "team-core"
created_at = "2026-07-29"
updated_at = "2026-07-29"
replaces = "none"
related_tasks = [
  "TASK-WRITE-0001", "TASK-WRITE-0002", "TASK-WRITE-0003", "TASK-WRITE-0004",
  "TASK-WRITE-0005", "TASK-WRITE-0006", "TASK-WRITE-0007", "TASK-WRITE-0008",
  "TASK-WRITE-0009", "TASK-WRITE-0010", "TASK-WRITE-0011", "TASK-WRITE-0012",
  "TASK-WRITE-0013", "TASK-WRITE-0014", "TASK-WRITE-0015", "TASK-WRITE-0016"
]
related_decisions = [
  "DEC-0001", "DEC-0002", "DEC-0003", "DEC-0004", "DEC-0005",
  "DEC-0006", "DEC-0007", "DEC-0008", "DEC-0009", "DEC-0010", "DEC-0011"
]
artifacts = ["crates/*", "proto/*", "docker/*", "config/*"]
validation = [
  "spike FlashDB FFI",
  "spike comparativo Opción A vs Opción B",
  "test de crash del writer (kill -9, 0 pérdidas)",
  "test de orden bajo concurrencia (10 writers, mismo entity_id)",
  "test de idempotencia (QoS 1 reintentos)",
  "benchmark de throughput (escrituras/segundo, lecturas/segundo)",
  "test de resync completo (100k registros)",
  "test de restore Litestream (RPO < 5s, RTO < 60s)"
]
```

## Metadata

- **ID**: SPEC-WRITE-0001
- **Estado**: `active`
- **Owner**: team-core
- **Fecha de creación**: 2026-07-29
- **Última actualización**: 2026-07-29
- **Reemplaza**: `none`
- **Tareas relacionadas**: `TASK-WRITE-0001` a `TASK-WRITE-0016`
- **Decisiones relacionadas**: `DEC-0001` a `DEC-0011`

## Resumen

Crear un motor en Rust que permita a múltiples backends/pods escribir en una misma instancia de SQLite sin contención, usando un canal de ingesta desacoplado (MQTT o FlashDB), un único writer serializado, una cache de lectura (FlashDB), y backup continuo con Litestream.

## Problema

Múltiples backends necesitan escribir en SQLite, pero SQLite solo soporta un escritor simultáneo. Abrir conexiones de escritura desde cada backend provoca `SQLITE_BUSY`, reintentos, y degradación de latencia. No se quiere introducir un RDBMS distribuido (Postgres, MySQL) por costo y complejidad operativa.

## Objetivo

Los backends envían escrituras sin preocuparse por concurrencia, bloqueos, o pérdida de datos. El sistema garantiza orden por entidad, idempotencia, y ofrece dos modos de confirmación (async/sync) seleccionables por request. Las lecturas se sirven desde cache con fallback a SQLite. La recuperación ante desastres está automatizada con Litestream.

## Alcance

**Incluye**:
- Canal de ingesta de escrituras desacoplado (spike para decidir entre Mosquitto y FlashDB como buffer).
- Writer único que aplica escrituras en SQLite con batching, deduplicación, y `BEGIN IMMEDIATE`.
- API REST + gRPC para escritura y consulta de estado.
- Dos modos de confirmación: `ack=accepted` (async, devuelve `write_id`) y `ack=committed` (sync, espera commit vía correlación).
- Endpoint `GET /writes/{idempotency_key}` para consulta de estado en modo async.
- Cache de lectura (FlashDB o alternativa) con TTL, invalidación por writer, y fallback automático a SQLite.
- Comando offline de reconstrucción de cache (`ixmati-resync`).
- Litestream hacia ≥2 destinos con RPO < 5s y RTO < 60s.
- Health checks integrados.
- Métricas (lag de cola, latencia de commit, tasa de misses).

**Excluye**:
- Replicación multi-master o escritura concurrente directa a SQLite.
- Sharding horizontal (múltiples archivos SQLite).
- Autenticación/autorización de negocio (responsabilidad de capas superiores).
- Failover automático del writer (manual u orquestado externamente).
- CDC para suscriptores externos (backlog).

## Requisitos funcionales

- **RF-1**: la API acepta escrituras vía REST (JSON) y gRPC (protobuf) con el envelope definido en `CONVENTIONS.md`.
- **RF-2**: el writer consume mensajes del canal de ingesta y aplica escrituras en SQLite de forma serializada.
- **RF-3**: el writer soporta operaciones `upsert`, `delete`, y `patch`.
- **RF-4**: el writer rechaza mensajes duplicados (`idempotency_key` ya procesada) con `DUPLICATE`.
- **RF-5**: el writer rechaza mensajes con versión obsoleta (`version <= stored_version`) con `VERSION_CONFLICT`.
- **RF-6**: el modo `ack=accepted` devuelve ack inmediato al recibir el mensaje.
- **RF-7**: el modo `ack=committed` devuelve ack solo tras commit exitoso en SQLite.
- **RF-8**: existe un endpoint `GET /writes/{idempotency_key}` que devuelve el estado de una escritura async.
- **RF-9**: las lecturas se sirven desde cache (FlashDB o alternativa). Si hay miss, se consulta SQLite y se repuebla la cache.
- **RF-10**: el writer invalida o repuebla la cache tras cada commit exitoso.
- **RF-11**: el comando `ixmati-resync` reconstruye la cache desde SQLite.
- **RF-12**: Litestream replica el WAL a destinos configurados de forma continua.

## Requisitos no funcionales

- **RNF-1**: 0 mensajes perdidos ante `kill -9` del writer y reinicio (mensajes retenidos en Mosquitto o durabilidad equivalente).
- **RNF-2**: 0 errores `SQLITE_BUSY` propagados al cliente en condiciones normales.
- **RNF-3**: p99 de latencia de ack en modo async < 50ms.
- **RNF-4**: orden de escrituras preservado por entidad+id (mismo `entity` + `key` → mismo orden de aplicación).
- **RNF-5**: read-your-writes garantizado en modo `committed`.
- **RNF-6**: RPO < 5 segundos, RTO < 60 segundos en disaster recovery vía Litestream.
- **RNF-7**: la API soporta ≥ 1000 escrituras/segundo sostenidas sin degradación.
- **RNF-8**: la API soporta ≥ 500 lecturas/segundo sostenidas desde cache sin degradación.
- **RNF-9**: el resync de cache procesa ≥ 50k registros/segundo.
- **RNF-10**: todos los errores de escritura se loguean con `idempotency_key`, `entity`, `key`, `op`, `version`, `latency_ms`.

## Criterios de aceptación

- **CA-1 — Escritura concurrente sin bloqueos**: dado 10 backends escribiendo concurrentemente sobre entidades distintas, ninguna escritura recibe `SQLITE_BUSY` ni error de timeout.
- **CA-2 — Crash del writer sin pérdida**: dado el writer procesando mensajes, cuando se ejecuta `kill -9` y se reinicia, entonces 0 mensajes se pierden (todos los publicados antes del crash son aplicados tras el reinicio).
- **CA-3 — Orden preservado por entidad**: dados 2 backends escribiendo sobre el mismo `entity/key` con versiones 1, 2, 3, cuando se consulta SQLite, entonces los datos reflejan la versión más alta y no hay regresiones.
- **CA-4 — Idempotencia**: dado un mensaje con `idempotency_key=X`, cuando se reenvía el mismo mensaje 3 veces, entonces solo se aplica una vez y las otras 2 reciben `DUPLICATE`.
- **CA-5 — Rechazo de versión obsoleta**: dado un registro con `version=5`, cuando se envía un mensaje con `version=3`, entonces se recibe `VERSION_CONFLICT` y el registro no se modifica.
- **CA-6 — Modo async**: dado un mensaje con `ack_mode=accepted`, cuando se envía, entonces se recibe ack en < 50ms (p99) y el `write_id` es consultable vía `GET /writes/{idempotency_key}`.
- **CA-7 — Modo sync con read-your-writes**: dado un mensaje con `ack_mode=committed`, cuando se recibe ack exitoso, entonces una lectura inmediata del mismo `entity/key` devuelve los datos recién escritos.
- **CA-8 — Cache miss con fallback**: dado que la cache no tiene un registro, cuando se consulta, entonces se lee de SQLite, se guarda en cache, y se retorna correctamente.
- **CA-9 — Invalidación de cache tras escritura**: dado un registro en cache, cuando el writer aplica una escritura sobre el mismo `entity/key`, entonces la siguiente lectura obtiene el valor actualizado (no el cacheado).
- **CA-10 — Resync completo**: dado SQLite con 100k registros y la cache borrada, cuando se ejecuta `ixmati-resync`, entonces la cache se reconstruye completamente en < 5 segundos.
- **CA-11 — Restore de backup**: dado un backup de Litestream en S3, cuando se ejecuta `litestream restore`, entonces SQLite se recupera con RPO < 5s y el servicio arranca en < 60s.

## Dependencias y riesgos

- **Dependencia**: Mosquitto como broker MQTT (si se elige Opción A). Requiere configuración de persistence y QoS.
- **Dependencia**: FlashDB vía FFI (riesgo DEC-0009). Si no es viable, migrar a sled/redb/lmdb-rs.
- **Dependencia**: Litestream para replicación WAL. Requiere acceso a S3 o filesystem remoto.
- **Riesgo**: el writer es single-node; si cae, las escrituras se detienen hasta que se reinicie (Mosquitto retiene mensajes en disco).
- **Riesgo**: FlashDB en servidor Linux puede no ofrecer las mismas propiedades que en embebidos. El spike lo determina.

## Plan de ejecución

Ver `TASKS.md` para el desglose completo de tareas y `ROADMAP.md` para las fases.

1. Spike de viabilidad y decisión (Fase 0): FlashDB FFI, comparativa A/B, contratos.
2. Core del writer (Fase 1): `ixmati-core`, `ixmati-writer`, tests de crash.
3. API (Fase 2): REST + gRPC, modos async/sync.
4. Cache (Fase 3): `ixmati-cache`, invalidación, `ixmati-resync`.
5. Producción (Fase 4): Litestream, health checks, runbook.
6. Observabilidad (Fase 5): métricas, backpressure, alertas, benchmarks.

## Plan de validación

- Spike FlashDB FFI (`TASK-WRITE-0001`): compilar + benchmark sintético vs sled.
- Spike comparativo A/B (`TASK-WRITE-0002`): test de crash, medición de latencia, orden bajo concurrencia.
- Test de crash del writer (`TASK-WRITE-0007`): script que publica N mensajes, `kill -9` el writer, reinicia, y verifica que los N mensajes están en SQLite.
- Test de idempotencia: publicar el mismo mensaje 3 veces, verificar 1 sola aplicación.
- Test de orden: 2 productores concurrentes, versiones 1..100 intercaladas, verificar orden final en SQLite.
- Benchmark de throughput: medir escrituras/segundo y lecturas/segundo del sistema completo.
- Test de resync: borrar cache, ejecutar `ixmati-resync`, verificar integridad.
- Test de restore Litestream: simular pérdida de datos, ejecutar restore, verificar RPO/RTO.

## Trazabilidad

- **Commits o PRs**: pendiente
- **Archivos principales**: `crates/*`, `proto/*`, `docker/*`, `config/*`
- **Resultado de validación**: pendiente

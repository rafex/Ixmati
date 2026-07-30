# DECISIONS.md

Registro de decisiones persistentes del proyecto.

## Decisiones firmes (`accepted`)

### DEC-0001 — SQLite como única fuente de verdad con WAL + synchronous=NORMAL

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0005`, `TASK-WRITE-0006`
- **Contexto**: se necesita una base de datos transaccional, ACID, sin servidor externo, que pueda correr en un VPS pequeño o en un pod. PostgreSQL o MySQL requieren un proceso separado, aumentan la complejidad operativa y el costo de infraestructura. SQLite es autocontenido, no requiere administración, y con WAL soporta lecturas concurrentes sin bloquear al escritor.
- **Decisión**: SQLite es la fuente de verdad canónica. Se configura con `PRAGMA journal_mode=WAL`, `PRAGMA synchronous=NORMAL`, `PRAGMA busy_timeout=5000`, `PRAGMA foreign_keys=ON`. Solo el writer abre la base de datos en modo escritura. Las lecturas de fallback (miss de cache) abren conexiones en modo solo lectura.
- **Consecuencias**:
  - (+) Sin servidor de base de datos externo. El binario y el archivo `.db` son todo lo necesario.
  - (+) WAL permite lecturas concurrentes sin bloquear la escritura.
  - (+) `synchronous=NORMAL` reduce la latencia de escritura sin riesgo de corrupción (el WAL protege la integridad).
  - (-) Un solo escritor por diseño. No se puede escalar horizontalmente la escritura.
  - (-) `synchronous=NORMAL` puede perder la última transacción en caso de crash del SO (no corrupción, solo pérdida de lo no sincronizado). Aceptable para el RPO declarado (< 5s vía Litestream).
- **Reemplaza**: `none`

### DEC-0002 — Un solo escritor lógico; nadie más abre la DB en write

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0006`
- **Contexto**: SQLite solo soporta un escritor a la vez. Intentar escribir desde múltiples procesos genera `SQLITE_BUSY` y reintentos que degradan la latencia. La alternativa de usar `busy_timeout` con reintentos no escala bien más allá de 2-3 escritores concurrentes.
- **Decisión**: solo el proceso `ixmati-writer` abre la base de datos con permisos de escritura. Ningún otro componente —ni siquiera la API— abre SQLite en modo write. Esto garantiza serialización predecible y elimina la contención.
- **Consecuencias**:
  - (+) Sin `SQLITE_BUSY` jamás. La serialización es explícita y controlada.
  - (+) El writer puede hacer batching y optimizaciones sin preocuparse por concurrencia.
  - (-) El writer es un punto único de fallo para escrituras (mitigado por Mosquitto con persistence).
  - (-) No se puede escalar horizontalmente el Writer (pero sí los backends y los lectores).
- **Reemplaza**: `none`

### DEC-0003 — Idempotencia con `idempotency_key` + `version`/`updated_at`

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0005`, `TASK-WRITE-0006`
- **Contexto**: en un sistema con QoS 1 (at least once), los mensajes pueden entregarse más de una vez. Además, múltiples backends pueden intentar escribir sobre el mismo registro. Se necesita un mecanismo para garantizar que cada mensaje se aplica exactamente una vez y que las versiones obsoletas se rechazan.
- **Decisión**:
  1. Todo mensaje incluye `idempotency_key` (UUID v4). El writer mantiene una tabla `_idempotency` con `(key, applied_at, status)`. Los mensajes con key ya existente se rechazan con `DUPLICATE`.
  2. Todo mensaje incluye `version` (u64 monotónico). El writer rechaza `version <= stored_version` con `VERSION_CONFLICT`.
  3. La tabla `_idempotency` tiene TTL de 24h (limpieza periódica).
  4. Resolución de conflictos: last-write-wins basado en `version`; a igual `version`, gana `ts` más reciente.
- **Consecuencias**:
  - (+) Reintentos seguros sin efectos secundarios.
  - (+) Protección contra escrituras fuera de orden (versiones obsoletas).
  - (+) QoS 1 es suficiente (no se necesita QoS 2, que duplica la latencia).
  - (-) La tabla `_idempotency` crece con cada escritura. Necesita limpieza periódica.
  - (-) Last-write-wins no es adecuado para conflictos mergeables (ej. contadores). Si surge ese caso, se evaluará CRDT en una decisión futura.
- **Reemplaza**: `none`

### DEC-0004 — Orden por particionado de topic `entity/id`, no FIFO global

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0006`
- **Contexto**: si el orden de escrituras importa (ej. actualizaciones consecutivas sobre el mismo registro), un FIFO global serializaría todas las escrituras en un solo worker, anulando cualquier paralelismo. Se necesita una solución que preserve el orden donde importa sin sacrificar el throughput.
- **Decisión**: los topics MQTT siguen el formato `ixmati/write/<entity>/<id>`. Múltiples workers pueden consumir topics diferentes, pero todos los mensajes para una misma entidad+id van al mismo worker (porque MQTT garantiza orden dentro de un mismo topic). Esto permite paralelismo entre entidades distintas y orden garantizado dentro de la misma entidad.
- **Consecuencias**:
  - (+) Paralelismo real: workers independientes para entidades distintas.
  - (+) Orden garantizado donde importa (mismo id).
  - (-) Si todas las escrituras son sobre el mismo `id`, no hay paralelismo (es un solo worker). Caso poco probable en la práctica.
  - (-) La topología de topics puede crecer; Mosquitto debe manejar muchos topics pequeños.
- **Reemplaza**: `none`

### DEC-0005 — Batching con `BEGIN IMMEDIATE` acotado por tamaño/tiempo

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0006`
- **Contexto**: escribir cada mensaje en su propia transacción genera mucha contención de journal y reduce el throughput. Agrupar mensajes en batches mejora significativamente el rendimiento. Se necesita una estrategia de batching que no introduzca latencia excesiva y que use el modo de transacción correcto.
- **Decisión**:
  1. El writer acumula mensajes hasta que se alcanza `MAX_BATCH_SIZE` (default: 100) o `MAX_BATCH_INTERVAL_MS` (default: 50ms), lo que ocurra primero.
  2. El batch se ejecuta dentro de `BEGIN IMMEDIATE` (no `BEGIN` a secas) para evitar que SQLite reintente automáticamente, lo cual es innecesario porque solo hay un escritor.
  3. Si el batch falla por `SQLITE_BUSY` (no debería ocurrir, pero como defensa), se reintenta 3 veces con backoff exponencial (10ms, 100ms, 1s).
- **Consecuencias**:
  - (+) Throughput de escritura mejorado en ~10x respecto a una transacción por mensaje.
  - (+) `BEGIN IMMEDIATE` evita que SQLite haga upgrade de shared a reserved lock (innecesario con un solo escritor).
  - (-) 50ms de latencia adicional máxima en el peor caso (mensaje solitario esperando a que se llene el batch o expire el timer).
  - (-) Si un mensaje en el batch falla (ej. VERSION_CONFLICT), el batch entero podría fallar. Mitigación: pre-validar versiones antes del commit y excluir mensajes conflictivos del batch.
- **Reemplaza**: `none`

### DEC-0006 — Cache (FlashDB o alternativa) siempre reconstruible desde SQLite

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0011`, `TASK-WRITE-0013`
- **Contexto**: la cache de lectura (FlashDB o alternativa) puede corromperse, llenarse, o desincronizarse de SQLite. No es aceptable que una corrupción de cache cause pérdida de datos o requiera intervención manual compleja.
- **Decisión**: FlashDB es siempre tratado como un artefacto desechable. Existe un comando `ixmati-resync` que lo reconstruye completamente desde SQLite en frío (offline). La cache se puede borrar y reconstruir en cualquier momento sin pérdida de datos. En caso de fallo de cache, el sistema automáticamente hace fallback a SQLite para lecturas.
- **Consecuencias**:
  - (+) Recuperación trivial: borrar el archivo de cache y ejecutar resync.
  - (+) Sin riesgo de pérdida de datos por corrupción de cache.
  - (+) El formato interno de FlashDB puede cambiar sin migraciones (se reconstruye).
  - (-) Durante el resync, las lecturas van a SQLite, aumentando la carga temporalmente.
  - (-) Si la cache se borra en caliente, hay una ventana de latencia aumentada hasta que se repuebla.
- **Reemplaza**: `none`

### DEC-0007 — Litestream a ≥2 destinos con RPO < 5s y RTO < 60s

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0014`
- **Contexto**: un solo archivo SQLite en un VPS es vulnerable a fallo de disco, corrupción del filesystem, o pérdida del VPS. Se necesita backup continuo con bajo RPO y recuperación rápida.
- **Decisión**:
  1. Litestream replica el WAL de SQLite a ≥2 destinos: un bucket S3-compatible (o R2, o MinIO) y un filesystem en otro VPS.
  2. RPO objetivo < 5 segundos (intervalo de sync configurable).
  3. RTO objetivo < 60 segundos (tiempo desde detección hasta servicio restaurado).
  4. El comando de restore está documentado en `COMMANDS.md` y probado en un runbook.
- **Consecuencias**:
  - (+) Recuperación point-in-time con granularidad de segundos.
  - (+) Sin impacto en el rendimiento de escritura (Litestream lee el WAL, no compite por el write lock).
  - (-) Costo de storage adicional en S3 + VPS remoto.
  - (-) En failover, el último batch (no replicado aún) puede perderse. Ventana máxima: `sync-interval`.
- **Reemplaza**: `none`

### DEC-0008 — Semántica de escritura dual: async (`accepted`) y sync (`committed`), seleccionable por request

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0009`, `TASK-WRITE-0010`
- **Contexto**: diferentes operaciones tienen diferentes requisitos de consistencia. Una actualización de perfil puede tolerar consistencia eventual (async), pero una deducción de saldo requiere confirmación de commit (sync). Imponer un solo modo obliga al caso más restrictivo para todos, aumentando la latencia innecesariamente.
- **Decisión**:
  1. El campo `ack_mode` en el envelope acepta `accepted` (async) o `committed` (sync).
  2. Modo `accepted`: el writer publica ack inmediato al recibir el mensaje. El backend recibe un `write_id` y puede consultar `GET /writes/{idempotency_key}` para verificar el estado.
  3. Modo `committed`: el writer retiene la conexión del backend (o publica en un topic de respuesta) hasta que el commit en SQLite se completa, y entonces envía el ack con el resultado.
  4. La correlación de respuestas usa el `idempotency_key` como identificador de request.
- **Consecuencias**:
  - (+) Latencia mínima para escrituras que no requieren confirmación inmediata.
  - (+) Read-your-writes garantizado en modo sync.
  - (+) El backend elige el nivel de consistencia por operación.
  - (-) Mayor complejidad en la API (dos caminos de respuesta).
  - (-) El modo sync introduce latencia de extremo a extremo (RTT API → MQTT → Writer → SQLite → Writer → API).
  - (-) Si el writer cae durante un commit sync, el backend debe manejar el timeout y reintentar (la idempotencia lo protege).
- **Reemplaza**: `none`

### DEC-0009 — Riesgo abierto: FlashDB requiere FFI; criterio de salida a sled/redb/lmdb-rs

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0001`
- **Contexto**: FlashDB es una librería C diseñada para microcontroladores (STM32, ESP32). No tiene binding oficial en Rust. Integrarlo requiere FFI con `bindgen` + `cc`, lo cual introduce complejidad de build, riesgos de seguridad (unsafe), y posible incompatibilidad con el modelo de memoria de servidor.
- **Decisión**: se intentará integrar FlashDB vía FFI en el spike `TASK-WRITE-0001`. Si el spike revela alguno de estos bloqueos, se descarta FlashDB y se adopta una alternativa (sled, redb, o lmdb-rs):
  1. El FFI no compila en Linux x86_64 (el target de servidor, no de microcontrolador).
  2. La API de FlashDB no soporta TTL o invalidación selectiva.
  3. El rendimiento en servidor es peor que sled/redb en benchmarks sintéticos.
  4. La superficie de `unsafe` es inmanejable (> 100 líneas o requiere invariantes complejos).
- **Consecuencias**:
  - (+) No hay bloqueo: si FlashDB no funciona, hay alternativas maduras en Rust puro.
  - (+) El trait `CacheBackend` abstrae la implementación, haciendo el cambio transparente.
  - (-) Tiempo invertido en el spike de FlashDB que podría ser tiempo perdido si se descarta (1-3 días).
- **Reemplaza**: `none`

---

## Decisiones abiertas (`proposed`)

Estas decisiones dependen del resultado del spike `TASK-WRITE-0002`. Se actualizarán a `accepted` cuando haya evidencia experimental.

### DEC-0010 — Canal de ingesta de escrituras: Mosquitto vs FlashDB como buffer

- **Fecha**: 2026-07-29
- **Estado**: `proposed`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0002`
- **Contexto**: la arquitectura tiene dos opciones para el canal de ingesta de escrituras. La Opción A usa Mosquitto como cola persistente (el backend publica, el writer consume). La Opción B usa FlashDB como buffer directo (el backend escribe en FlashDB, el worker lee y aplica a SQLite). Ambas tienen trade-offs distintos en durabilidad, latencia, y complejidad. Ver `ARCHITECTURE.md` para diagramas y tabla comparativa.
- **Decisión pendiente**: se elegirá la opción que ofrezca el mejor balance entre durabilidad y latencia, basado en los experimentos del spike.
- **Criterio de cierre**:
  - Resultados del spike `TASK-WRITE-0002`: durabilidad ante `kill -9`, latencia p50/p99, orden bajo concurrencia.
  - Si Opción B pierde > 0 mensajes en el test de crash, se descarta automáticamente (no cumple requisito de 0 pérdidas).
  - Si Opción B no preserva orden por entidad, se descarta automáticamente.
- **Reemplaza**: `none`

### DEC-0011 — Camino de lectura: directo vs vía cola

- **Fecha**: 2026-07-29
- **Estado**: `proposed`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0002`
- **Contexto**: la Opción A propone lectura directa (cache → miss → SQLite → repoblar). La Opción B propone lectura vía cola (API → Mosquitto → worker → SQLite → FlashDB → retorno). La Opción B añade una cola a cada lectura, lo cual puede congestionar el sistema si hay muchas lecturas.
- **Decisión pendiente**: se elegirá lectura directa a menos que la Opción B demuestre una ventaja clara en algún escenario.
- **Criterio de cierre**:
  - Latencia p99 de lectura en ambas opciones bajo carga (100 lecturas/segundo concurrentes).
  - Crecimiento de FlashDB en Opción B (¿explosión de entradas por consultas distintas?).
  - Si la Opción B tiene p99 > 2x la Opción A, se descarta.
- **Reemplaza**: `none`

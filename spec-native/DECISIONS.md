# DECISIONS.md

Registro de decisiones persistentes del proyecto.

## Decisiones firmes (`accepted`)

### DEC-0001 — SQLite como única fuente de verdad con WAL + synchronous=NORMAL

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Enmienda**: 2026-07-29 — un SQLite **por store**
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0005`, `TASK-WRITE-0006`, `TASK-WRITE-0017`
- **Contexto**: se necesita una base de datos transaccional, ACID, sin servidor externo, que pueda correr en un VPS pequeño o en un pod. PostgreSQL o MySQL requieren un proceso separado, aumentan la complejidad operativa y el costo de infraestructura. SQLite es autocontenido, no requiere administración, y con WAL soporta lecturas concurrentes sin bloquear al escritor.
- **Decisión**: SQLite es la fuente de verdad canónica. Se configura con `PRAGMA journal_mode=WAL`, `PRAGMA synchronous=NORMAL`, `PRAGMA busy_timeout=5000`, `PRAGMA foreign_keys=ON`. Cada store tiene su propio archivo SQLite. Solo el writer de ese store abre la base de datos en modo escritura. Las lecturas de fallback (miss de cache) y los proyectores abren conexiones en modo solo lectura.
- **Consecuencias**:
  - (+) Sin servidor de base de datos externo. Los binarios y los archivos `.db` son todo lo necesario.
  - (+) WAL permite lecturas concurrentes sin bloquear la escritura.
  - (+) `synchronous=NORMAL` reduce la latencia de escritura sin riesgo de corrupción (el WAL protege la integridad).
  - (+) N stores = N escritores independientes, cada uno con su propio WAL y sin contención entre stores.
  - (-) Un solo escritor por store por diseño. No se puede escalar horizontalmente la escritura *dentro* de un store.
  - (-) `synchronous=NORMAL` puede perder la última transacción en caso de crash del SO (no corrupción, solo pérdida de lo no sincronizado). Aceptable para el RPO declarado (< 5s vía Litestream).
- **Reemplaza**: `none`

### DEC-0002 — Un solo escritor por store; nadie más abre la DB en write

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Enmienda**: 2026-07-29 — "un solo escritor **por store**" (antes: "nadie más abre la DB en write" global)
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0006`, `TASK-WRITE-0014`
- **Contexto**: SQLite solo soporta un escritor a la vez por archivo. Intentar escribir desde múltiples procesos genera `SQLITE_BUSY` y reintentos que degradan la latencia.
- **Decisión**: solo el proceso writer asignado a un store abre el archivo SQLite de ese store con permisos de escritura. Ningún otro componente abre SQLite en modo write. Esto garantiza serialización predecible y elimina la contención. Con múltiples stores, hay múltiples writers independientes (uno por store) que no compiten entre sí.
- **Consecuencias**:
  - (+) Sin `SQLITE_BUSY` jamás. La serialización es explícita y controlada por store.
  - (+) Los writers de distintos stores son independientes y paralelos.
  - (+) El writer puede hacer batching y optimizaciones sin preocuparse por concurrencia externa.
  - (-) El writer es un punto único de fallo para escrituras de su store (mitigado por Mosquitto con persistence).
  - (-) No se puede escalar horizontalmente un writer individual (pero sí los backends y los lectores).
  - (-) Renombrar un store implica el equivalente a una migración de base de datos (nuevo archivo, nueva replicación Litestream).
- **Reemplaza**: `none`

### DEC-0003 — Idempotencia con `idempotency_key` + `version`/`updated_at`

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Enmienda**: 2026-07-29 — scope de la clave pasa a `(store, idempotency_key)`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0005`, `TASK-WRITE-0006`
- **Contexto**: en un sistema con QoS 1 (at least once), los mensajes pueden entregarse más de una vez. Además, múltiples backends pueden intentar escribir sobre el mismo registro. Se necesita un mecanismo para garantizar que cada mensaje se aplica exactamente una vez y que las versiones obsoletas se rechazan.
- **Decisión**:
  1. Todo comando incluye `store` (obligatorio), `idempotency_key` (UUID v4), y `version` (u64 monotónico).
  2. El writer mantiene una tabla `_idempotency` por store con `(key, applied_at, status)`. Los comandos con key ya existente en ese store se rechazan con `DUPLICATE`.
  3. El writer rechaza `version <= stored_version` dentro del mismo store con `VERSION_CONFLICT`.
  4. La tabla `_idempotency` tiene TTL de 24h (limpieza periódica).
  5. Resolución de conflictos: last-write-wins basado en `version`; a igual `version`, gana `ts` más reciente.
- **Consecuencias**:
  - (+) Reintentos seguros sin efectos secundarios.
  - (+) Protección contra escrituras fuera de orden (versiones obsoletas).
  - (+) QoS 1 es suficiente (no se necesita QoS 2, que duplica la latencia).
  - (-) La tabla `_idempotency` crece con cada escritura. Necesita limpieza periódica.
  - (-) Last-write-wins no es adecuado para conflictos mergeables (ej. contadores). Si surge ese caso, se evaluará CRDT en una decisión futura.
- **Reemplaza**: `none`

### DEC-0004 — Orden por particionado jerárquico `store/entity/id`, no FIFO global

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Enmienda**: 2026-07-29 — jerarquía `store → entity → id`; 1 proceso writer por store con concurrencia interna
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0006`, `TASK-WRITE-0017`
- **Contexto**: si el orden de escrituras importa (ej. actualizaciones consecutivas sobre el mismo registro), un FIFO global serializaría todas las escrituras en un solo worker, anulando cualquier paralelismo. Se necesita una solución que preserve el orden donde importa sin sacrificar el throughput.
- **Decisión**: los topics de comando siguen el formato `ixmati/cmd/<store>/<entity>/<id>`. Cada store tiene exactamente **un proceso writer** (1 pod). El paralelismo vive dentro del proceso: el writer consume de múltiples topics del mismo store concurrentemente, pero serializa los de la misma entidad+id. Nunca hay 2 pods sobre el mismo archivo SQLite.
- **Consecuencias**:
  - (+) Paralelismo entre stores (N writers independientes).
  - (+) Concurrencia interna dentro del writer por entidad (parallelismo intra-store).
  - (+) Orden garantizado dentro de `(store, entity, id)`.
  - (-) Si todas las escrituras son sobre el mismo `(store, entity, id)`, el throughput se reduce a 1 worker para ese flujo. Inevitable si el orden importa.
- **Reemplaza**: `none`

### DEC-0005 — Batching con `BEGIN IMMEDIATE` acotado por tamaño/tiempo

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0006`
- **Contexto**: escribir cada mensaje en su propia transacción genera mucha contención de journal y reduce el throughput. Agrupar mensajes en batches mejora significativamente el rendimiento.
- **Decisión**:
  1. El writer acumula comandos por store hasta `MAX_BATCH_SIZE` (default: 100) o `MAX_BATCH_INTERVAL_MS` (default: 50ms), lo que ocurra primero.
  2. El batch se ejecuta dentro de `BEGIN IMMEDIATE`. La transacción incluye: los datos de la entidad, la tabla `_idempotency`, y la tabla `_outbox` (ver DEC-0014).
  3. Si el batch falla por `SQLITE_BUSY` (no debería ocurrir, pero como defensa), se reintenta 3 veces con backoff exponencial (10ms, 100ms, 1s).
- **Consecuencias**:
  - (+) Throughput de escritura mejorado en ~10x respecto a una transacción por mensaje.
  - (+) `BEGIN IMMEDIATE` evita que SQLite haga upgrade de shared a reserved lock.
  - (-) 50ms de latencia adicional máxima en el peor caso.
  - (-) Si un mensaje en el batch falla (VERSION_CONFLICT), se excluye y el resto del batch se aplica.
- **Reemplaza**: `none`

### DEC-0006 — Cache y read models siempre reconstruibles desde los stores

- **Fecha**: 2026-07-29
- **Estado**: `superseded`
- **Reemplazada por**: `DEC-0019` y `DEC-0020`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Contexto original**: la cache de lectura (FlashDB o alternativa) puede corromperse y debe ser desechable. Con la introducción de stores y eventos, la cache se complementa con read models proyectados, que también deben ser reconstruibles.
- **Decisión original**: FlashDB es tratado como artefacto desechable con comando de resync.
- **Razón del reemplazo**: el modelo de reconstrucción cambia de "dumping de 1 store" a "fan-in sobre N stores vía eventos". Las decisiones `DEC-0019` y `DEC-0020` cubren este caso superset.

### DEC-0007 — Litestream a ≥2 destinos por store con RPO < 5s y RTO < 60s

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Enmienda**: 2026-07-29 — **por store**, con criticidad y frecuencia independientes
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0014`, `TASK-WRITE-0024`
- **Contexto**: un solo archivo SQLite en un VPS es vulnerable a fallo de disco, corrupción del filesystem, o pérdida del VPS. Con múltiples stores, cada uno puede tener requisitos de criticidad distintos (ej. `pedidos` necesita RPO más bajo que `logs`).
- **Decisión**:
  1. Litestream replica el WAL de cada store a ≥2 destinos: un bucket S3-compatible (o R2, o MinIO) y un filesystem en otro VPS.
  2. Por store: frecuencia (`sync-interval`), destinos y retención configurables independientemente.
  3. RPO objetivo < 5 segundos para stores críticos.
  4. RTO objetivo < 60 segundos (tiempo desde detección hasta servicio restaurado).
  5. Cada store tiene su propia instancia de Litestream (sidecar en K8s, o proceso separado).
- **Consecuencias**:
  - (+) Recuperación point-in-time con granularidad de segundos por store.
  - (+) Aislamiento: una corrupción en un store no afecta el backup de otros.
  - (-) Costo de storage adicional en S3 + VPS remoto multiplicado por N stores.
  - (-) N procesos de Litestream (uno por store) → mayor consumo de recursos.
- **Reemplaza**: `none`

### DEC-0008 — Semántica de escritura dual: async (`accepted`) y sync (`committed`), seleccionable por request

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Enmienda**: 2026-07-29 — el ack sync está acotado a 1 store; **invariante: un comando toca exactamente 1 store**
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0009`, `TASK-WRITE-0010`
- **Contexto**: diferentes operaciones tienen diferentes requisitos de consistencia. Una actualización de perfil puede tolerar consistencia eventual (async), pero una deducción de saldo requiere confirmación de commit (sync).
- **Decisión**:
  1. El campo `ack_mode` en el comando acepta `accepted` (async) o `committed` (sync).
  2. Modo `accepted`: ack inmediato al recibir el mensaje. El backend puede consultar `GET /writes/{store}/{idempotency_key}`.
  3. Modo `committed`: el writer retiene la conexión hasta el commit en SQLite, y envía el ack con el resultado.
  4. La correlación usa `(store, idempotency_key)`.
  5. **Invariante**: un comando apunta a 1 store. Si una operación de negocio requiere tocar 2 stores, la aplicación debe emitir 2 comandos separados y coordinar (saga fuera del motor).
- **Consecuencias**:
  - (+) Latencia mínima para escrituras que no requieren confirmación inmediata.
  - (+) Read-your-writes garantizado en modo sync, acotado al store del comando.
  - (+) Sin transacciones distribuidas en el motor.
  - (-) El backend debe coordinar operaciones multi-store por su cuenta.
- **Reemplaza**: `none`

### DEC-0009 — Riesgo abierto: FlashDB requiere FFI; criterio de salida a sled/redb/lmdb-rs

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Enmienda**: 2026-07-29 — severidad sube: FlashDB pasará a alojar **read models proyectados**, no solo cache. Perderlo cuesta más.
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0001`
- **Contexto**: FlashDB es una librería C diseñada para microcontroladores. No tiene binding oficial en Rust. Con la introducción de read models proyectados, FlashDB deja de ser solo una cache volátil — contiene datos desnormalizados que tardan en reconstruirse. La fiabilidad del backend de storage sube de importancia.
- **Decisión**: se intentará integrar FlashDB vía FFI en `TASK-WRITE-0001`. Si el spike revela alguno de estos bloqueos, se descarta FlashDB y se adopta una alternativa (sled, redb, o lmdb-rs):
  1. El FFI no compila en Linux x86_64.
  2. La API no soporta TTL o invalidación por prefijo.
  3. El rendimiento es peor que sled/redb en benchmarks sintéticos.
  4. La superficie de `unsafe` es inmanejable (> 100 líneas o requiere invariantes complejos).
- **Consecuencias**:
  - (+) El trait `CacheBackend` abstrae la implementación.
  - (-) Tiempo invertido en el spike que podría ser tiempo perdido.
  - (-) Si FlashDB falla, hay que migrar read models a otro backend; el costo de migración es mayor que con cache pura.
- **Reemplaza**: `none`

### DEC-0010 — Canal de ingesta de escrituras: Mosquitto (Opción A)

- **Fecha**: 2026-07-29
- **Estado**: `accepted` (cerrada por diseño, antes `proposed`)
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0006`, `TASK-WRITE-0017`
- **Contexto**: la arquitectura tenía dos opciones para el canal de ingesta de escrituras. Con la introducción de transactional outbox (`DEC-0014`) y la taxonomía separada de comandos y eventos (`DEC-0015`), la Opción B (FlashDB como buffer de escritura) queda descartada por incompatibilidad de diseño, no por benchmark.
- **Decisión**: Mosquitto con persistence y QoS 1 es el canal de ingesta para comandos. Motivos de descarte de FlashDB como buffer:
  1. No soporta el patrón outbox: necesitarías dos escrituras atómicas (dato + evento) y FlashDB no es transaccional.
  2. El orden de eventos no está garantizado en FlashDB sin un mecanismo externo de secuenciación.
  3. La durabilidad ante crash es inferior (DEC-0009 ya marca el riesgo de FFI; añadir escrituras en tránsito lo agrava).
- **Consecuencias**:
  - Ver consecuencias originales de DEC-0001 y DEC-0002.
  - `TASK-WRITE-0002` (spike comparativo A vs B) se cancela.
- **Reemplaza**: `none`

### DEC-0011 — Camino de lectura: directo (cache → miss → SQLite → repoblar)

- **Fecha**: 2026-07-29
- **Estado**: `accepted` (cerrada por diseño, antes `proposed`)
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0011`, `TASK-WRITE-0020`
- **Contexto**: la Opción B proponía lectura vía cola (API → Mosquitto → worker → SQLite → FlashDB → retorno). Con la introducción de read models proyectados eager (`DEC-0020`), la cola de lecturas pierde todo sentido.
- **Decisión**: lectura directa con cache-aside + read models opt-in.
  1. El backend consulta FlashDB con prefijo `c:<store>:<entity>:<key>` (cache-aside) o `p:<projection>:<key>` (read model).
  2. Si hay miss en ambos, se consulta SQLite del store correspondiente.
  3. La cola de lecturas (MQTT) no existe. Las proyecciones se alimentan del bus de eventos, no de requests de lectura.
- **Consecuencias**:
  - (+) Latencia de lectura mínima (sin paso por cola).
  - (+) Sin explosión de entradas en FlashDB por consultas distintas.
  - (+) Las proyecciones se actualizan en escritura, no en lectura.
- **Reemplaza**: `none`

---

### DEC-0012 — Store como primitivo del motor

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0017`
- **Contexto**: la propuesta introduce "SQLite por dominio", pero el motor no debe asumir DDD. Necesita un primitivo que funcione igual con 1 store que con N, independientemente de si los stores representan dominios, tenants, regiones o particiones temporales.
- **Decisión**:
  1. Un **store** es la tupla `(archivo SQLite, un writer, un prefijo de topic, un destino Litestream)`.
  2. Es la unidad de serialización, transaccionalidad, backup y aislamiento de fallo.
  3. `stores=1` es el caso degenerado y debe funcionar sin ningún componente adicional (sin bus de eventos, sin outbox, sin proyectores).
  4. "Dominio" es una etiqueta de config (`label: "pedidos"`). El código solo conoce stores.
  5. Topología: configurable entre `N` pods (aislamiento K8s, 1 pod por store) y 1 proceso con `N` writer tasks (single-VPS, el supervisor lanza 1 task por store).
- **Consecuencias**:
  - (+) El motor sirve para DDD y para cualquier otro criterio de partición.
  - (+) `stores=1` no paga sobrecarga de eventos ni outbox.
  - (-) La topología de despliegue es una decisión operativa que el config debe expresar.
  - (-) Renombrar un store es una migración: nuevo archivo, nueva replicación, migración de topics.
- **Reemplaza**: `none`

### DEC-0013 — Sin transacciones cross-store; sagas fuera del motor

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0005` (envelope con `store` obligatorio)
- **Contexto**: con N stores independientes, no hay forma de hacer una transacción ACID que abarque dos stores (SQLite no soporta transacciones distribuidas). El motor debe establecer un límite claro de responsabilidad.
- **Decisión**:
  1. Un comando toca exactamente 1 store. El campo `store` en el envelope es obligatorio.
  2. La aplicación que necesite atomicidad cross-store debe implementar sagas u otro patrón de coordinación.
  3. El motor provee las primitivas necesarias para construir sagas: outbox transaccional (`DEC-0014`), idempotencia (`DEC-0003`), y entrega at-least-once de eventos.
  4. Explícitamente fuera de alcance: orquestador de sagas, compensación automática, estado de saga.
- **Consecuencias**:
  - (+) Límite claro: el motor no se convierte en un coordinador de transacciones distribuidas.
  - (+) Las primitivas provistas son suficientes para que cualquier aplicación construya sagas encima.
  - (-) La aplicación debe manejar la complejidad de coordinación multi-store.
- **Reemplaza**: `none`

### DEC-0014 — Transactional outbox

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0018`
- **Contexto**: cuando un comando se aplica en SQLite, los proyectores necesitan saberlo para actualizar sus read models. Si el writer publica el evento *después* del commit, y el proceso muere entre ambos, el evento se pierde para siempre y los read models quedan desincronizados en silencio — el peor modo de fallo.
- **Decisión**:
  1. Cada store tiene una tabla `_outbox` con columnas: `id INTEGER PRIMARY KEY`, `event_type TEXT`, `event_id TEXT UNIQUE`, `store TEXT`, `entity TEXT`, `key TEXT`, `version INTEGER`, `occurred_at TEXT`, `payload JSON`, `published_at TEXT NULL`.
  2. El evento se inserta en `_outbox` **dentro de la misma transacción** que los datos de la entidad y la tabla `_idempotency`. Esto garantiza atomicidad: o se escriben ambos (dato + evento), o ninguno.
  3. El **publicador** es una task interna del proceso writer. Lee `_outbox WHERE published_at IS NULL`, publica en `ixmati/evt/<store>/<entity>/<id>` con QoS 1, y marca `published_at` con el timestamp de publicación.
  4. La marca de `published_at` es idempotente (si MQTT ack falla, se reintenta). La entrega es at-least-once. Los consumidores (proyectores) deben ser idempotentes.
- **Consecuencias**:
  - (+) 0 eventos perdidos. El outbox sobrevive a crash del writer.
  - (+) El publicador es parte del writer → no viola la invariante de escritor único.
  - (+) Sin dependencia externa para publicar eventos (no se necesita Debezium ni triggers).
  - (-) La tabla `_outbox` crece. Necesita limpieza periódica (filas con `published_at` anterior a N días).
  - (-) Latencia adicional entre commit y publicación (el publicador sondea o usa notificaciones). Típicamente < 100ms.
- **Reemplaza**: `none`

### DEC-0015 — Taxonomía de topics separada: comandos vs eventos

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0003`, `TASK-WRITE-0019`
- **Contexto**: comandos y eventos tienen semántica, consumidores y ciclo de vida distintos. Mezclarlos en el mismo árbol de topics causa acoplamiento indeseado y dificulta la operación.
- **Decisión**:

| | Comando | Evento |
|---|---|---|
| Topic | `ixmati/cmd/<store>/<entity>/<id>` | `ixmati/evt/<store>/<entity>/<id>` |
| Semántica | "haz esto" | "esto pasó" |
| Consumidores | 1 (el writer del store) | N (proyectores, auditoría) |
| Se puede rechazar | Sí (`VERSION_CONFLICT`, `DUPLICATE`) | No (es un hecho consumado) |
| Retención en Mosquitto | Hasta consumo + TTL | Mayor (permitir reproyección) |
| `retained` | `false` | `false` (los proyectores se ponen al día desde outbox) |

- **Consecuencias**:
  - (+) Separación clara de responsabilidades.
  - (+) Se pueden configurar políticas de retención distintas.
  - (+) Los proyectores no se ven afectados por rechazos de comandos (ni siquiera los ven).
- **Reemplaza**: `none`

### DEC-0016 — Read models: patrón R por defecto, M por excepción

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0020`, `TASK-WRITE-0021`
- **Contexto**: los read models pueden materializar datos de otros stores de dos formas. Llamarlas igual ("join en FlashDB") oculta que tienen costos operativos radicalmente distintos.
- **Decisión**:
  - **Patrón R (referencia + lookup)**: el read model guarda `{usr_id: 9}`. La consulta completa hace 2 lecturas a FlashDB (`ped:123` + `usr:9`) y combina en Rust. Escritura barata (sin fan-out), 2 lecturas, sin propagación de cambios.
  - **Patrón M (materializado)**: el read model guarda `{usr_id: 9, usr_nombre: "Ana"}`. Una sola lectura. Pero al renombrar "Ana" → "Ana M.", el proyector debe reescribir todas las filas que referencian ese usuario.
  - **Regla de decisión**: usar M solo si `fan_out ≤ 100` Y `ratio_lectura_escritura ≥ 100:1` para el campo copiado. Por defecto, R.
- **Consecuencias**:
  - (+) La regla de decisión es explícita y medible, no opinable.
  - (+) R es seguro por defecto: sin riesgo de fan-out descontrolado.
  - (-) Quien declara la proyección debe conocer los datos de acceso para aplicar la regla.
- **Reemplaza**: `none`

### DEC-0017 — ATTACH DATABASE permitido solo en lectura

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0023`
- **Contexto**: `ATTACH DATABASE` permite a SQLite hacer consultas entre múltiples archivos. Es la única forma razonable de hacer reporting cross-store sin mover datos a otro sistema. Pero mal usado, rompe la invariante de escritor único.
- **Decisión**:
  1. **Prohibido** para cualquier conexión que tenga permisos de escritura. Sin excepción.
  2. **Permitido** en conexiones de solo lectura, sobre réplicas co-localizadas (mismo nodo), exclusivamente para reporting, analítica ad-hoc y debugging.
  3. No se usa en el camino de lectura online (ese va por FlashDB o SQLite directo).
- **Consecuencias**:
  - (+) Escape hatch para consultas analíticas sin montar un data warehouse.
  - (+) Sin riesgo para la integridad de los stores (solo lectura).
  - (-) Requiere disciplina: el `ATTACH` debe estar bloqueado a nivel de permisos de archivo o de PRAGMA.
- **Reemplaza**: `none`

### DEC-0018 — Keyspace de FlashDB unificado con prefijo `<store>:<entity>:<key>`

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0011`, `TASK-WRITE-0020`
- **Contexto**: FlashDB aloja tanto cache-aside como read models. Con múltiples stores y proyecciones, el keyspace debe ser predecible, purgable por prefijo, y sin colisiones entre stores.
- **Decisión**:
  1. Keyspace unificado con prefijos:
     - Cache-aside: `c:<store>:<entity>:<key>`
     - Read models (proyecciones): `p:<projection_name>:<key>`
  2. Esto permite purgar toda la cache de un store (`c:pedidos:*`) o una proyección entera (`p:pedidos_con_usuario:*`) sin afectar otras.
  3. Un solo archivo FlashDB para todos los stores y proyecciones. Alternativa: un archivo por store si un store domina el volumen (configurable).
- **Consecuencias**:
  - (+) Purga quirúrgica por prefijo.
  - (+) Sin colisiones entre stores ni entre cache y proyecciones.
  - (-) Un solo archivo = un solo punto de fallo para todas las lecturas (mitigado: la cache es desechable y hay fallback a SQLite).
- **Reemplaza**: `none`

### DEC-0019 — Reconciler: reproyección fan-in sobre N stores

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0022`
- **Contexto**: con read models proyectados desde eventos, la reconstrucción ya no es "dumping de 1 store" (DEC-0006 original), sino "fan-in sobre N stores". Si un read model se corrompe o necesita migrarse, hay que reproyectarlo desde los datos canónicos de los stores fuente.
- **Decisión**:
  1. El `ixmati-reconciler` es un binario offline que reconstruye read models.
  2. Soporta dos modos: reproyección total (todos los read models desde cero) y selectiva (`--projection <name>`).
  3. Modo fan-in: para cada proyección, lee los stores fuente en modo solo lectura, aplica la lógica de proyección (R o M), y escribe en FlashDB.
  4. La cache-aside (`c:*`) no se reconstruye (se repuebla con el tráfico). Solo los read models (`p:*`).
- **Consecuencias**:
  - (+) Recuperación de read models sin depender del bus de eventos (offline).
  - (+) Permite migrar el esquema de una proyección: se borra `p:<name>:*` y se reproyecta.
  - (-) Lectura intensiva de todos los stores fuente durante la reproyección.
  - (-) La lógica de proyección está duplicada (en el projector online y en el reconciler offline). Mitigación: compartir el código de proyección en `ixmati-core`.
- **Reemplaza**: `DEC-0006`

### DEC-0020 — Coexistencia cache-aside + proyecciones

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0011`, `TASK-WRITE-0020`, `TASK-WRITE-0021`
- **Contexto**: no todos los datos necesitan proyección. La mayoría de las lecturas se benefician de cache-aside simple. Imponer proyecciones para todo añadiría complejidad innecesaria. Pero para ciertos patrones de acceso (lecturas frecuentes que cruzan stores), las proyecciones eager son superiores.
- **Decisión**:
  1. El motor soporta ambos caminos de lectura simultáneamente:
     - **Cache-aside** (lazy, por defecto): `c:<store>:<entity>:<key>` en FlashDB. Se llena en read-miss desde SQLite. Se invalida/actualiza por el writer tras cada commit.
     - **Proyección** (eager, opt-in): `p:<projection_name>:<key>` en FlashDB. Se llena por el projector al recibir eventos. Se declara explícitamente en config.
  2. Ambos comparten el mismo `CacheBackend` con namespaces distintos.
  3. Una proyección puede declarar su propio TTL (inferior al de cache-aside, porque se actualiza proactivamente).
  4. El backend elige qué camino usar por consulta (implícito: si pide por `projection`, va a proyección; si pide por `store/entity/key`, va a cache-aside).
- **Consecuencias**:
  - (+) Sin overhead de proyecciones para datos que no las necesitan.
  - (+) Las proyecciones se añaden incrementalmente sin rediseñar.
  - (-) Dos caminos de lectura que el backend debe entender (mitigado: la API unifica la interfaz).
  - (-) La purga de cache-aside no debe afectar proyecciones y viceversa (resuelto por prefijos).
- **Reemplaza**: `DEC-0006` (junto con `DEC-0019`)

---

### DEC-0021 — make = build, just = tasks; just→make permitido, make→just prohibido

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-TOOL-0001`
- **Relacionado con tareas**: `TASK-TOOL-0003`, `TASK-TOOL-0004`, `TASK-TOOL-0005`
- **Contexto**: el proyecto necesita build y task runner. Ambos deben coexistir sin ambigüedad sobre sus responsabilidades.
- **Decisión**: `make` construye artefactos (codegen, compilación, Docker, dist). `just` es el task manager (test, lint, fmt, hooks, docs, CI). `just` puede invocar `make`. `make` **nunca** puede invocar `just`. El guard es `helpers/python/lint_tool_boundary.py`, que escanea `Makefile` y `helpers/make/*.mk` y falla si encuentra `just`.
- **Consecuencias**: (+) Separación clara. (+) Guard automatizado, no solo convención. (-) Si make necesita algo que solo just sabe hacer, hay que mover la lógica a helpers/shell/ o helpers/python/.
- **Reemplaza**: `none`

### DEC-0022 — `helpers/` como única fuente de lógica compartida

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-TOOL-0001`
- **Relacionado con tareas**: `TASK-TOOL-0002`, `TASK-TOOL-0004`, `TASK-TOOL-0005`
- **Contexto**: `Makefile` y `Justfile` necesitan lógica compartida (preflight, esperas, manejo de MQTT). Duplicarla en ambos archivos causa divergencia.
- **Decisión**: toda la lógica compartida vive en `helpers/` y solo allí. `Makefile` incluye `helpers/make/*.mk`. `Justfile` importa `helpers/just/*.just`. Ambos invocan `helpers/shell/*.sh` y `helpers/python/*.py`. `Makefile` y `Justfile` en raíz son thin: solo incluyen/importan, no contienen lógica.
- **Consecuencias**: (+) Una sola fuente de verdad para cada script. (+) Si make necesita algo que está en just, se extrae a helpers. (-) Hay que mantener la disciplina de no poner lógica en los archivos raíz.
- **Reemplaza**: `none`

### DEC-0023 — Python tooling con `uv`, Python ≥3.12; nunca el intérprete del sistema

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-TOOL-0001`
- **Relacionado con tareas**: `TASK-TOOL-0001`
- **Contexto**: el sistema operativo puede tener Python 3.9 o inferior. Los scripts necesitan features de 3.12 (`X | None` en types) y dependencias gestionadas.
- **Decisión**: `uv` gestiona Python ≥3.12 y las dependencias del proyecto. `pyproject.toml` en `helpers/python/` con `requires-python = ">=3.12"`. `.python-version` = `3.12`. Los scripts usan shebang `#!/usr/bin/env uv run`. Nunca se usa `python3` del sistema.
- **Consecuencias**: (+) Python correcto garantizado en cualquier máquina. (+) Lock file versionado. (-) `uv` es prerequisito adicional.
- **Reemplaza**: `none`

### DEC-0024 — Tiers de test: unitarios in-source, integración (crate Rust), smoke (pytest)

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-TOOL-0001`, `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-TOOL-0007`, `TASK-TOOL-0008`, `TASK-TOOL-0009`
- **Contexto**: los tests deben organizarse por alcance y herramienta adecuada. Unitarios no necesitan infraestructura. Integración necesita acceso a APIs internas de los crates. Smoke necesita orquestar procesos (docker, kill -9).
- **Decisión**: 1) Unitarios: `#[cfg(test)]` in-source en cada crate. 2) Integración: crate Rust `tests/integration/` miembro del workspace (`publish = false`), accede a APIs públicas de los crates. 3) Smoke: pytest sobre uv en `tests/smoke/`, caja negra contra docker compose. Fixtures compartidos en `tests/fixtures/`.
- **Consecuencias**: (+) Cada tier usa la herramienta correcta. (+) Smoke puede hacer `kill -9`, esperar puertos y verificar SQLite sin depender de Rust. (-) Dos lenguajes de test (Rust + Python), pero el overlap es mínimo.
- **Reemplaza**: `none`

### DEC-0025 — TDD con ratchet de cobertura desde el día 1

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-TOOL-0001`, `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-TOOL-0007`, `TASK-TOOL-0010`
- **Contexto**: sin un mecanismo de cobertura, el proyecto tiende a perder cobertura sin notarlo. TDD sin gate de cobertura es aspiracional, no aplicable.
- **Decisión**: 1) Bootstrap TDD: cada crate nace con un test que falla (`assert_eq!(2+2, 5)`). Solo se corrige al implementar. 2) Ratchet: `.coverage-floor` versionado contiene el piso mínimo. `coverage_gate.py` falla si la cobertura baja y sugiere subir el piso si el margen ≥ 0.5pp. El piso arranca en 0.0 y solo sube. 3) El gate corre en `just test-cov-gate` y en `just ci-main`. 4) `cargo-llvm-cov` como herramienta de medición.
- **Consecuencias**: (+) Cobertura monotónicamente creciente. (+) Bootstrap TDD visible: `just test-unit` falla hasta que se implementa. (-) El primer commit post-bootstrap requiere corregir los 7 tests rojos.
- **Reemplaza**: `none`

### DEC-0026 — Hooks versionados en `.githooks/` vía `core.hooksPath`

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-TOOL-0001`
- **Relacionado con tareas**: `TASK-TOOL-0006`
- **Contexto**: los git hooks no se versionan en `.git/hooks/`. Se necesita un mecanismo para que todos los contribuidores ejecuten las mismas verificaciones sin configuración manual.
- **Decisión**: hooks en `.githooks/` en la raíz. `just hooks-install` ejecuta `git config core.hooksPath .githooks`. `just hooks-uninstall` revierte. Los hooks delegan su lógica a `helpers/` (shell y python), no contienen lógica inline. Pre-commit: fmt + clippy rápido + boundary + validate config. Commit-msg: Conventional Commits. Pre-push: unit + integration.
- **Consecuencias**: (+) Hooks versionados y revisables en PR. (+) Instalación en un solo comando. (-) `core.hooksPath` es local (no se pushea). Cada dev debe ejecutar `just hooks-install` una vez.
- **Reemplaza**: `none`

### DEC-0027 — Separación `docs/` (consumidor) vs `spec-native/` (contribuidor)

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-TOOL-0001`
- **Relacionado con tareas**: `TASK-TOOL-0012`
- **Contexto**: los documentos del proyecto sirven a dos audiencias distintas que no deben mezclarse.
- **Decisión**: `docs/` (mdBook, HTML publicable) documenta cómo **usar y operar** Ixmati. Audiencia: quien integra la herramienta y quien la opera en producción. `spec-native/` documenta **por qué** está construido así. Audiencia: agentes y contribuidores. No se duplica contenido entre ambas. El runbook de producción vive en `docs/src/operations/runbook.md`. Los gates de CI/CD viven en `spec-native/pipelines/CI.md` y `CD.md`.
- **Consecuencias**: (+) Cada audiencia encuentra su contenido sin ruido. (+) mdBook genera sitio navegable. (-) Hay que mantener la disciplina de no cruzar las fronteras.
- **Reemplaza**: `none`

---

### DEC-0028 — Podman rootless como runtime único de contenedores

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-CONTAINERS-0001`
- **Contexto**: el host de producción es un Linux amd64 con Podman 5.4.2 rootless. Docker no está disponible ni es necesario.
- **Decisión**: Podman rootless es el único runtime de contenedores soportado. Target `linux/amd64`. Builds ejecutan nativamente en el host remoto vía túnel SSH. En CI se usa podman compose en vez del bloque `services:`.
- **Consecuencias**: (+) Un solo runtime en todos los entornos. (+) Builds nativos amd64 sin QEMU. (-) El túnel SSH debe estar activo para cualquier operación de contenedor.
- **Reemplaza**: `none`

### DEC-0029 — `containers/` una carpeta por imagen, `Containerfile` + symlink

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-CONTAINERS-0001`
- **Decisión**: `containers/` como raíz. Una carpeta por imagen con `Containerfile` (idiomático Podman) + symlink `Dockerfile → Containerfile` para compatibilidad.
- **Reemplaza**: `none`

### DEC-0030 — Builder cargo-chef + runtime `debian:trixie-slim`

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-CONTAINERS-0001`
- **Decisión**: Builder único con cargo-chef compila los 5 binarios una vez. Runtime `debian:trixie-slim` (glibc, seguro para FlashDB FFI). Musl diferido hasta cerrar DEC-0009.
- **Reemplaza**: `none`

### DEC-0031 — Compose dev/test, Quadlet producción

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-CONTAINERS-0001`
- **Decisión**: `podman compose` para dev/test. Quadlet (systemd rootless) para producción. `ixmati-writer@.container` como unit templada: `systemctl --user start ixmati-writer@pedidos`.
- **Reemplaza**: `none`

### DEC-0032 — Puertos por rango funcional con registro

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-CONTAINERS-0001`
- **Decisión**: Web 30000-30099, API 30100-30199, DB 30200-30299, Temp 30300-30399. Mosquitto en DB (es almacenamiento persistente). Reservados documentados: 30080, 30081, 30083, 30300.
- **Reemplaza**: `none`

### DEC-0033 — Builds nativos amd64 vía túnel SSH + `.containerignore`

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Relacionado con specs**: `SPEC-CONTAINERS-0001`
- **Decisión**: Builds siempre contra el remoto. Guard `podman_remote.sh` aborta si target ≠ amd64. `.containerignore` excluye `target/`, `.git/`, `docs/book/`.
- **Reemplaza**: `none`

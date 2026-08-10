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

### DEC-0009 — RESUELTO: FlashDB descartado. SQLite cache adoptado como default.

- **Fecha**: 2026-07-29
- **Estado**: `replaced` por DEC-0012
- **Fecha de resolucion**: 2026-08-05
- **Resultado del spike**:

| Criterio | Resultado | Veredicto |
|---|---|---|
| FFI compila en Linux x86_64 | ✅ Compila con libclang-dev + bindgen | OK |
| API soporta TTL e invalidacion | ✅ delete_by_prefix funciona via iter FFI | OK |
| Rendimiento vs alternativas | ❌ GET p50=1107µs (SQLite=14µs, Redb=26µs) | FAIL |
| Superficie unsafe manejable | ⚠️ 228 lineas de unsafe FFI | RISK |
| Persistencia en disco | ❌ No escribe archivos en file mode | FAIL |
| Multi-proceso | ❌ Diseñado para microcontroladores | FAIL |

- **Causa raiz del fallo**: FlashDB (`FDB_USING_FILE_LIBC_MODE`) no persiste datos en Linux. `fdb_kv_set_blob` reporta `FDB_NO_ERR` pero no crea archivos. Ademas, `fdb_kv_get_blob` nunca retorna datos. El rendimiento de lectura (1107µs p50) es 80x peor que SQLite (14µs p50).

- **Decision**: Se descarta FlashDB. Se adopta **SQLite cache** (`SqliteCacheBackend`) como backend default para cache-aside. Redb queda como alternativa single-process cuando crates.io tenga >= 4.5 con `ReadOnlyDatabase`.

- **Reemplaza**: `none`

- **Reemplazado por**: DEC-0012 (SQLite cache como default)

---

### DEC-0012 — SQLite cache como backend default para cache-aside

- **Fecha**: 2026-08-05
- **Estado**: `superseded`
- **Reemplazada por**: DEC-0036
- **Contexto**: DEC-0009 descarto FlashDB. Se necesita un backend de cache que funcione multi-proceso en el all-in-one (supervisord: API + writer como procesos separados).
- **Decision**: Usar SQLite con tabla `_cache` como backend de cache. Archivo separado (`cache.db`) con WAL para acceso concurrente.
- **Alternativas evaluadas**:

| Backend | GET p50 | Multi-proceso | Persistencia | Veredicto |
|---|---|---|---|---|
| SQLite (WAL) | 14µs | ✅ | ✅ | **Elegido** |
| Redb 4.1.0 | 26µs | ❌ (sin ReadOnlyDatabase) | ✅ | Esperar 4.5+ |
| FlashDB 2.2.0 | 1107µs | ❌ | ❌ | Descartado |

- **Consecuencias**:
  - (+) Cero dependencias nuevas (rusqlite ya bundled)
  - (+) Mismo ecosistema que la BD principal (backup con Litestream)
  - (+) WAL permite lecturas concurrentes API + writer sin bloqueos
  - (+) Cache hit verificado en E2E (`"source":"cache"`)
  - (+) Metricas Prometheus (`CACHE_HITS`, `CACHE_MISSES`)
  - (-) No es un cache "puro" (es otra tabla SQLite)
  - (-) Podria crecer si no se limpia periodicamente
- **Reemplaza**: DEC-0009
- **Razon del reemplazo**: DEC-0036 demostro que Redb con transporte socket es superior en rendimiento multi-proceso, y el socket transport elimina la limitacion de ReadOnlyDatabase que bloqueaba a Redb en DEC-0012.

---

### DEC-0036 — Redb como backend de cache de produccion via socket transport

- **Fecha**: 2026-08-05
- **Estado**: `accepted`
- **Contexto**: DEC-0012 eligio SQLite como default porque redb 4.1.0 no soportaba acceso multi-proceso sin `ReadOnlyDatabase`. Las pruebas de carga posteriores demostraron que el transporte socket (Unix domain socket IPC entre API y writer) resuelve esa limitacion: la API consulta al writer via socket, el writer opera Redb en modo read-write exclusivo, y la API nunca abre el archivo Redb directamente. Esto permite usar Redb en entornos multi-proceso sin esperar redb 4.5+.
- **Decision**: Redb es el backend de cache de produccion por defecto (`CACHE_BACKEND=redb`). El modo de lectura por defecto es socket (`CACHE_READ_MODE=socket`) con Unix domain socket IPC. SQLite permanece como fallback (`CACHE_BACKEND=sqlite` con `CACHE_READ_MODE=direct`).
- **Resultados de carga** (1000 lecturas concurrentes, Redb como backend):

| Modo | p50 | Ratio vs socket |
|---|---|---|
| Redb + socket | 17.59ms | 1.00x (baseline) |
| Redb + direct (ReadOnlyCache) | 39.00ms | 2.22x mas lento |
| Redb + MQTT | ~68ms | ~3.9x mas lento |

- **Alternativas evaluadas**:

| Backend | GET p50 (directo) | Multi-proceso | Soporte socket | Veredicto |
|---|---|---|---|---|
| Redb 4.1.0 + socket | 26µs | ✅ (via writer) | ✅ | **Elegido** |
| SQLite (WAL) | 14µs | ✅ | N/A (directo) | Fallback |
| FlashDB 2.2.0 | 1107µs | ❌ | ❌ | Descartado |

- **Consecuencias**:
  - (+) Redb + socket es 2.2x mas rapido que acceso directo bajo carga concurrente
  - (+) El transporte socket mantiene al writer como dueno unico del archivo Redb (no rompe DEC-0002)
  - (+) Arquitectura preparada para migrar a redb 4.5+ con `ReadOnlyDatabase` (eliminaria el overhead del socket en modo directo)
  - (+) SQLite cache se mantiene como fallback operativo con solo cambiar `CACHE_BACKEND=sqlite`
  - (-) El socket añade ~17ms de overhead vs acceso single-process teorico (~26µs)
  - (-) El writer debe estar vivo para servir lecturas de cache (mitigado: smoke tests verifican disponibilidad)
- **Reemplaza**: DEC-0012

---

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

### DEC-0034 — DEC-0025: Smoke tests requieren SSH port forwarding al podman remoto

- Fecha: 2026-08-04
- Estado: `accepted`
- Relacionado con specs:
- Contexto: El podman usado en desarrollo es remoto (192.168.3.175 vía túnel SSH en :18081). Los contenedores del stack smoke.yaml exponen puertos en la máquina remota, no en localhost. Sin port forwarding explícito, los tests Python no pueden alcanzar mosquitto (30310) ni la API (30311). El firewall del remoto bloquea acceso externo a esos puertos.
- Decisión: Se usan SSH port forwards adicionales (-L 30310:localhost:30310 -L 30311:localhost:30311) para exponer los puertos del stack smoke en localhost. El conftest.py resuelve el host vía SMOKE_HOST env var (default: extraído de SSH_HOST en podman_tunnel.sh, fallback: localhost). El stack compose se ejecuta via podman socket (túnel :18081) pero las conexiones MQTT/HTTP van por los port forwards.
- Consecuencias: - Los smoke tests E2E requieren dos túneles SSH activos: uno para podman socket (:18081) y otro para puertos de servicio (:30310, :30311)
- El script podman_tunnel.sh debería extenderse para incluir estos port forwards automáticamente (tarea pendiente)
- Si se migra a podman local (podman machine en macOS), SMOKE_HOST=localhost funcionaría sin port forwards adicionales
- Los tests standalone (test_smoke.py) no requieren ningún túnel
- Reemplaza: none

### DEC-0035 — DEC-0026: .containerignore es obligatorio para builds remotos

- Fecha: 2026-08-04
- Estado: `accepted`
- Relacionado con specs:
- Contexto: El builder Containerfile usa COPY . . para transferir el contexto de build. Sin .containerignore, podman envía TODO el repositorio (incluyendo target/ con ~5GB de artefactos de compilación y .git/) a través del túnel SSH al podman remoto. Esto provocaba timeouts de >30min en el paso COPY del builder.
- Decisión: Se creó .containerignore excluyendo: target/, .git/, .venv/, __pycache__/, node_modules/, dist/, *.db*, .idea/, .vscode/, .DS_Store, *.log. Con esto el COPY tarda segundos y el build completo (chef cook + cargo build) se completa en ~2min.
- Consecuencias: - Todo Containerfile que use COPY . . desde el repo root requiere este .containerignore
- Si se añaden nuevos directorios con artefactos grandes, deben agregarse al .containerignore
- La tarea TASK-CONT-0001 (crear .containerignore) estaba marcada como [x] en TODO.md pero el archivo no existía — se creó ahora
- Reemplaza: none

### DEC-0037 — Cache-server dedicado como dueño único de Redb (socket-based)

- Fecha: 2026-08-06
- Estado: `accepted`
- Relacionado con specs:
- Contexto: La Fase 1 movió el acceso a Redb a un proceso cache-server dedicado, expuesto vía Unix socket. Writer, API, Projector y Reconciler se comunican con el cache-server vía CacheClient (socket), eliminando la necesidad de abrir Redb directamente desde múltiples procesos. Esto resuelve problemas de contención de escritura en Redb (single-writer) y unifica el keyspace de proyecciones bajo el prefijo `p:`.
- Decisión: 1. El cache-server es el único dueño de la base de datos Redb/SQLite de cache. 2. Toda lectura/escritura de proyecciones (`p:*`) y cache (`c:*`) pasa por el socket Unix. 3. El protocolo socket soporta GET, SET, DEL, DEL_PREFIX, FLUSH con framing HIT/MISS. 4. Los clientes (Writer, API, Projector, Reconciler) usan CacheClient para todas las operaciones de cache.
- Consecuencias: Beneficios: eliminación de contención Redb multi-proceso, keyspace unificado, API de proyecciones (`?projection=&key=`) funciona vía socket. Costos: latencia extra por salto de socket (~0.2ms en loopback), el cache-server es punto único de fallo para operaciones de cache, requiere healthcheck de socket en compose/supervisord.
- Reemplaza: none

### DEC-0038 — Validación e2e multi-store con compose + pytest smoke tests

- Fecha: 2026-08-06
- Estado: `accepted`
- Relacionado con specs:
- Contexto: La iniciativa projector-validation requería validar el flujo completo e-commerce: múltiples stores (pedidos, usuarios, inventario), proyecciones Pattern R y M, cache-server único vía socket, y concurrencia multi-proceso. Se implementaron tests e2e automatizados con pytest + podman compose.
- Decisión: 1. Tests e2e usan `multi-store.yaml` con podman compose (8 servicios: mosquitto, cache-server, api, 3 writers, projector, reconciler opcional). 2. Fixture `compose_up_multi` (session-scoped) gestiona el ciclo de vida del stack. 3. Puertos directos (MQTT 30200, API 30000) sin remapeo para simplificar. 4. Se agregó `http_read_projection(api, projection, key)` al harness de smoke tests. 5. Tests validan: materialized view (Pattern M), reference+lookup (Pattern R), idempotencia, y concurrencia con ThreadPoolExecutor.
- Consecuencias: Los tests e2e requieren `make containers-build` previo y `pytest -m e2e`. No se ejecutan en CI actual (requieren podman con capacidades de red). El fixture `compose_up_multi` soporta `E2E_EXTERNAL=1` para conectar a un stack ya levantado.
- Reemplaza: none

### DEC-0039 — Bugs detectados y resueltos en projector e2e: timeout + cross-store lookup + socket permissions

- Fecha: 2026-08-06
- Estado: `accepted`
- Relacionado con specs:
- Contexto: Durante la validación e2e del projector se encontraron 5 bugs que impedían el funcionamiento del pipeline completo: escritura → MQTT → projector → socket SET → cache-server → Redb → socket GET → API.
- Decisión: 1. Cache-server tenía `handle_client` con timeout de 30s en `read_line`, lo que cerraba la conexión del projector antes de que llegara el primer evento MQTT → se eliminó el timeout (las conexiones Unix socket no necesitan keepalive). 2. `process_r_async` usaba `event.entity`/`event.key` para cross-store lookups en vez de inferir la entidad desde el campo `{store}_id` del payload → se agregaron `resolve_lookup_key()` e `infer_entity_from_store()`. 3. `CacheClient::read_simple_response` tenía timeout de 20ms y descartaba respuestas silenciosamente → timeout aumentado a 5s, logging agregado. 4. Cache-server Containerfile usaba `EXPOSE 0` (inválido en podman) → eliminado. 5. `USER ixmati` en cache-server causaba `Permission denied` en el socket → cambiado a `USER 1000:1000` con `chmod 777`. 6. `MQTT_BROKER` vs `MQTT_HOST`/`MQTT_PORT` inconsistente entre servicios → unificado a `MQTT_BROKER=tcp://mosquitto:30200`. 7. `CACHE_READ_MODE=socket` y `IXMATI_API_KEYS` faltaban en compose → agregados.
- Consecuencias: Se agregaron 12 tests unitarios que cubren: composición de claves Redb (4 tests), cross-store lookup en pattern_r (4 tests), extracción de campos en pattern_m (3 tests), deserialización de ProjectionConfig con copy_fields (1 test). Tests existentes: 117 pasan. Tests e2e: 4/4 pasan (Pattern M, Pattern R, idempotencia, concurrencia). El stack multi-store con 7 servicios funciona correctamente con docker/podman compose. La API expone proyecciones vía `?projection=&key=`.
- Reemplaza: none

### DEC-0040 — Instalador nativo (systemd) incluye cache-server; env vars de projector/reconciler unificadas

- Fecha: 2026-08-09
- Estado: `accepted`
- Relacionado con specs: `SPEC-INSTALL-0001`
- Relacionado con tareas: `TASK-INST-0001`..`TASK-INST-0006`
- Contexto: el instalador nativo (`scripts/install.sh` + `helpers/python/installer.py`, empaquetado por `make dist`) predata a DEC-0037. Nunca se actualizó cuando `ixmati-cache-server` pasó a ser el dueño único de Redb: el binario no estaba en `BINARIES`, no tenía unidad systemd, y `start_services()` ni siquiera arrancaba `ixmati-projector`. Resultado: una instalación nativa limpia aceptaba escrituras pero no servía cache-aside ni proyecciones — no estaba realmente lista para usar. Además se detectó que `ixmati-projector` e `ixmati-reconciler` leían `IXMATI_MQTT_BROKER`/`IXMATI_CACHE_SOCKET` en vez de los nombres estándar `MQTT_BROKER`/`CACHE_SOCKET_PATH` usados por el resto de binarios, inconsistencia ya parcheada ad-hoc en `containers/compose/multi-store.yaml` pero no en la imagen all-in-one ni en el instalador nativo.
- Decisión: 1. Se renombraron las env vars de `ixmati-projector`/`ixmati-reconciler` en el código Rust para alinearlas con `MQTT_BROKER`/`CACHE_SOCKET_PATH` (default `/var/run/ixmati/cache.sock`), eliminando la necesidad de setear dos nombres distintos por servicio. 2. Nueva unidad `systemd/ixmati-cache-server.service` con `RuntimeDirectory=ixmati` (systemd crea `/run/ixmati` en cada arranque, evitando que el socket falle al bindear tras un reboot). 3. `ixmati-api.service`, `ixmati-writer@.service` e `ixmati-projector.service` ahora declaran `Requires=`/`After=ixmati-cache-server.service` y las env vars de cache/proyecciones que faltaban. 4. `installer.py`: `BINARIES`/`SYSTEMD_UNITS` incluyen cache-server, `start_services()` sigue el orden `mosquitto → cache-server → writer → api → projector` con espera activa del socket, `install_config()` ya no sobrescribe `stores.toml`/`projections.toml` si el usuario los editó, se agregó `verify_health()` (curl real a `/health` con reintentos) y modo `--uninstall`/`--uninstall --purge`. 5. `helpers/make/artifacts.mk` y `dist-validate.mk` incluyen `ixmati-cache-server` en binarios y unidades esperadas. 6. Nueva validación real en `containers/installer-test/` (Debian + systemd como PID 1) ejecutada vía `helpers/shell/test_installer_debian.sh` / `just installer-test`: instala, verifica los 5 servicios activos, hace round-trip write/read, reinstala (idempotencia) y desinstala con purge.
- Bugs adicionales encontrados por la validación real (no visibles por inspección estática, solo al correr el instalador de verdad en Debian): 7. `scripts/install.sh` invocaba `python3` sin verificar que existiera — un Debian mínimo no lo trae preinstalado → se agregó bootstrap de `python3` vía apt/dnf/yum antes de invocar `installer.py`, igual que ya se hacía para Mosquitto. 8. `configure_mosquitto()` copiaba `mosquitto.conf` como fragmento `/etc/mosquitto/conf.d/ixmati.conf`, pero el paquete Debian de Mosquitto ya define `persistence`/`persistence_location` en su config por defecto e `include_dir /etc/mosquitto/conf.d`; Mosquitto rechaza como fatal (`exit-code 3/NOTIMPLEMENTED`) un conf.d que duplique `persistence_location` → se cambió a reemplazar `/etc/mosquitto/mosquitto.conf` completo, con respaldo del original (`mosquitto.conf.pre-ixmati`) restaurado en `--uninstall`, y detección idempotente vía marcador `# ixmati-managed`. 9. `install_binaries()` fallaba con `OSError: [Errno 26] Text file busy` al reinstalar sobre binarios en ejecución (escribir in-place sobre el inode de un proceso activo) → se cambió a copiar a un temporal + `Path.replace()` (rename atómico), el mismo patrón que usan los gestores de paquetes reales.
- Consecuencias: (+) Una instalación nativa limpia queda funcionalmente completa (cache-aside y proyecciones funcionan sin pasos manuales). (+) Segunda ejecución de `install.sh` ya no pisa configuración editada por el usuario ni falla por binarios en uso. (+) Instalador soporta ciclo de vida completo (`install` / `install` reentrante / `uninstall --purge`), a la par de instaladores de PostgreSQL/MongoDB vía apt. (+) Validado end-to-end en `debian:trixie-slim` real (systemd como PID 1 en Podman `--privileged`): instalación limpia, reinstalación idempotente y desinstalación con purga, las 3 pasando con los 5 servicios activos y round-trip write/read exitoso. (-) El cambio de nombre de env vars es incompatible hacia atrás para cualquier despliegue externo que ya seteara `IXMATI_MQTT_BROKER`/`IXMATI_CACHE_SOCKET` manualmente (no había ningún despliegue de producción real al momento de esta decisión). (-) Al reinstalar la misma versión, los servicios ya activos no se reinician automáticamente para tomar el binario nuevo (`systemctl start` es no-op sobre un servicio activo) — un flujo real de upgrade entre versiones necesitará forzar `systemctl restart`; queda como mejora pendiente, no bloqueante para esta iniciativa. (-) El test de instalador en Debian requiere Podman `--privileged` con cgroups montados; no corre en el CI actual (mismo límite que los e2e de DEC-0038). (-) La validación se corrió con binarios `linux/arm64` (máquina Podman local de macOS, no el host remoto amd64 de DEC-0033); el instalador y las unidades systemd son agnósticas de arquitectura, pero no se validó el tarball `linux-amd64` real en esta pasada.
- Reemplaza: none

### DEC-0041 — Migración de Prometheus directo a OpenTelemetry en ixmati-api

- Fecha: 2026-08-09
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0005`
- Contexto: la validación de TASK-VAL-0004 en Debian real expuso que `crates/ixmati-api/src/metrics.rs` estaba acoplado directamente a la crate `prometheus` (`Registry::new_custom`, `register_*_with_registry!`), y que ese acoplamiento producía un bug real: `Registry::new_custom(Some("ixmati"))` sumado a nombres de métrica que ya empezaban con `ixmati_` generaba series duplicadas como `ixmati_ixmati_write_requests_total` en el endpoint `/metrics`. Además, atarse a Prometheus como único backend de métricas es inconsistente con el resto de la observabilidad del proyecto, que ya usa `tracing` (vendor-neutral) para logs.
- Decisión: 1. Se reemplazó la dependencia directa de `prometheus` en `ixmati-api` por `opentelemetry` + `opentelemetry_sdk` 0.32 (API de métricas vendor-neutral) con el exporter `opentelemetry-prometheus` 0.32, que sigue publicando `/metrics` en formato texto de Prometheus (sin romper scrapers existentes) pero permite añadir otros exporters (OTLP, etc.) sin tocar los call sites de instrumentación. 2. El namespace `"ixmati"` se configura una única vez en el exporter (`.with_namespace("ixmati")`), no en cada nombre de métrica — esto resuelve el bug de doble prefijo de raíz. 3. Los nombres de counters se declaran sin el sufijo `_total` (p.ej. `"write_requests"`) porque el exporter de Prometheus lo añade automáticamente; declararlo a mano hubiera producido `_total_total`. 4. Los 4 call sites reales en `rest.rs` (QUEUE_DEPTH, CACHE_HITS×2, CACHE_MISSES×2) se migraron de `.with_label_values(&[...]).set()/.inc()` a la API de atributos de OTel (`&[KeyValue::new(...)]` + `.record()`/`.add()`). 5. `prometheus` sigue como dependencia transitiva (vía el exporter) y se subió a 0.14 porque `opentelemetry-prometheus` 0.32 la requiere — el workspace pineaba 0.13.
- Consecuencias: (+) Ya no hay lock-in a Prometheus: cambiar de exporter no requiere tocar instrumentación en `rest.rs`. (+) Bug de doble prefijo resuelto de raíz (validado con test de regresión `metrics::tests::encode_metrics_has_single_ixmati_prefix_and_no_prometheus_dep_leak` y con tráfico real en Debian: `ixmati_cache_hits_total{namespace="cache",store="default",...}` sin duplicar). (+) El endpoint `/metrics` sigue siendo compatible con cualquier Prometheus/Grafana existente — no hay cambio de contrato hacia afuera salvo la corrección del bug. (-) El output de OTel agrega metadata adicional no presente antes: labels `otel_scope_name="ixmati-api"` en cada serie y una nueva serie `target_info{service_name=...}` — cosmético, estándar en cualquier pipeline OTel→Prometheus, pero es una diferencia de formato a tener en cuenta si hay dashboards con queries muy estrictas. (-) Persiste el gap ya documentado en TASK-VAL-0004: `WRITE_REQUESTS`, `WRITE_LATENCY`, `WRITE_ERRORS`, `OUTBOX_SIZE`, `PROJECTION_LAG` y las 3 métricas de proceso siguen sin call sites reales — esta migración cambió el mecanismo de instrumentación, no agregó la instrumentación faltante (fuera de alcance, queda como TASK-VAL-0006 potencial). (-) Solo se migró `ixmati-api`; ningún otro crate del workspace usaba `prometheus` directamente, así que no hay migración pendiente en otros crates.
- Reemplaza: none

### DEC-0042 — Instrumentación real de métricas OTel pendientes + hallazgo: el load test de bash mide el harness, no el servidor

- Fecha: 2026-08-09
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0006`
- Contexto: DEC-0041 migró la instrumentación a OpenTelemetry pero dejó documentado que 6 de 9 métricas (`write_requests_total`, `write_latency_seconds`, `write_errors_total`, `outbox_size`, `projection_lag_events`, `process_memory_rss_bytes`, `process_cpu_user_seconds_total`) seguían registradas sin call sites reales. Se completó esa instrumentación donde es genuinamente posible desde `ixmati-api`, y de paso el load test de `test_stack_validation.sh` (loop secuencial de `curl`, ~285 ops/s) se reemplazó por uno concurrente real.
- Decisión: 1. `write_handler` en `rest.rs` instrumenta `WRITE_REQUESTS` (éxito), `WRITE_ERRORS` (con `error_type` por cada camino de fallo: queue_full, serialize, mqtt_unavailable, mqtt_publish) y `WRITE_LATENCY` (siempre, éxito o error) usando `std::time::Instant` desde la entrada del handler. 2. Nuevo módulo `crates/ixmati-api/src/self_monitor.rs`, spawneado como tarea tokio de fondo antes de `axum::serve`, que cada 5s lee `/proc/self/status` (`VmRSS`) para `PROCESS_MEMORY_RSS` y `/proc/self/stat` (campo `utime`, delta vs. la lectura anterior, asumiendo `CLK_TCK=100`) para `PROCESS_CPU_USER`; si `SQLITE_PATH` está configurado, también consulta `_outbox WHERE published_at IS NULL` agrupado por store para `OUTBOX_SIZE`. 3. `PROJECTION_LAG` y `WRITE_BATCH_DURATION` quedan explícitamente sin instrumentar: sus datos viven en `ixmati-projector`/`ixmati-writer`, procesos que no exponen HTTP `/metrics` — instrumentarlos de verdad (no con datos inventados) requeriría agregar ese endpoint a esos binarios, decisión de mayor alcance que se deja pendiente. 4. `load_test()` en `helpers/shell/test_stack_validation.sh` se reescribió: en vez de un único loop secuencial de `curl`, lanza `LOAD_CONCURRENCY` (default 20) subshells en paralelo, cada uno escribiendo a su propio archivo de resultados, agregados con `wait` al finalizar.
- Hallazgo real durante la validación en Debian (`make installer-test` con binarios recompilados): con concurrencia 20 durante 30s se lograron 13460 writes (448.7 ops/s, contra 285 ops/s del loop secuencial), y `ixmati_write_requests_total` coincidió exactamente con el conteo del script (13460) — confirmando que la métrica ahora refleja tráfico real, no un placeholder. Sin embargo apareció una discrepancia de 4 órdenes de magnitud entre `WRITE_LATENCY` medida en servidor (~2.4 microsegundos promedio — solo cubre el tiempo de encolar el mensaje en el canal interno de `rumqttc::AsyncClient`, no un round-trip de red) y la latencia percibida por el script de carga (p50=37ms, p99=66ms, medida por `curl` vía HTTP). La causa no es el servidor: el harness de bash lanza un proceso `curl` nuevo por cada request (sin keep-alive, con costo de fork+exec+conexión TCP) y corre 20 de esos procesos en paralelo dentro del mismo contenedor, generando contención de CPU que domina la latencia observada. `OUTBOX_SIZE` se verificó forzando `SQLITE_PATH` vía un override temporal de systemd (el instalador nativo no lo setea por defecto, porque el API en `CACHE_READ_MODE=socket` no necesita SQLite directo salvo para esta métrica) y reveló un backlog real de 4762 eventos sin publicar tras la carga de 13460 writes — el writer no drenó la cola al ritmo de escritura del load test.
- Consecuencias: (+) Las métricas del write path ahora reflejan tráfico real y fueron verificadas contra el conteo independiente del script de carga, no solo "compila". (+) El backlog de outbox detectado (4762/13460 eventos, ~35%) es una señal real de que el writer es más lento que el throughput que el load test concurrente puede generar — dato útil para dimensionar el `batch_size`/`batch_interval_ms` del writer bajo carga sostenida, no investigado más a fondo en esta tarea. (-) Los números de "ops/s" y latencia del load test de `test_stack_validation.sh` miden el techo del harness de bash+curl (fork/exec, ausencia de keep-alive, contención de 20 procesos concurrentes), no la capacidad real de `ixmati-api` — no deben citarse como benchmark de producto sin esa salvedad. (-) `SQLITE_PATH` no está seteado por el instalador nativo por defecto, así que `OUTBOX_SIZE` queda inactivo en una instalación estándar salvo que se configure manualmente esa env var en la unidad systemd de `ixmati-api`. (-) `PROJECTION_LAG`/`WRITE_BATCH_DURATION` siguen sin datos reales; exponerlos requeriría un endpoint `/metrics` en `ixmati-projector`/`ixmati-writer`, cambio de mayor alcance no abordado aquí.
- Reemplaza: none

### DEC-0043 — Análisis de viabilidad: causa raíz del backlog de outbox (bug trivial) vs. techo real del cache-server (límite arquitectónico)

- Fecha: 2026-08-09
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: DEC-0042 dejó dos hallazgos sin investigar a fondo — el backlog de outbox (4762/13460 eventos, ~35%) y la brecha de latencia harness-vs-servidor. El usuario pidió determinar viabilidad del proyecto para un caso de uso de "alto volumen / multi-tenant" (miles de writes/s, múltiples tenants concurrentes). Se investigó con 2 agentes de exploración en paralelo (solo lectura): causa raíz del backlog en `ixmati-writer`, y límites estructurales de escalabilidad multi-tenant en el resto del stack.
- Decisión / hallazgos:
  1. **El backlog de outbox es un bug trivial, no un límite arquitectónico.** `crates/ixmati-writer/src/event_publisher.rs` drena el outbox con un `tokio::interval` de 1000ms hardcodeado (`main.rs:61`), publicando máximo 100 eventos por tick, también hardcodeado — techo teórico de ~100 eventos/s de salida. Peor: abre una nueva conexión SQLite por cada evento publicado solo para actualizar `published_at` de una fila (en vez de un `UPDATE ... WHERE id IN (...)` en batch), y publica secuencialmente con `.await` por evento sin `join_all`/pipelining. Con 448 writes/s de entrada medidos contra ~100 eventos/s de salida teórica, el backlog crece sin límite mientras dure la carga — exactamente lo observado en DEC-0042. El batcher de ingesta (`batcher.rs`) tampoco tiene backpressure: si el resto del pipeline se atrasa, el batch simplemente crece sin frenar la entrada. Fix de bajo esfuerzo (batch UPDATE + publicación concurrente + intervalo/límite configurables) probablemente lleva el drenado de ~100/s a varios miles/s sin tocar el diseño de single-writer-por-store.
  2. **El cache-server centralizado SÍ es un techo arquitectónico real para multi-tenant de alto volumen.** `crates/ixmati-cache/src/cache_server.rs` + `redb_backend.rs`: el backend Redb usa `std::sync::Mutex<Database>` — un único lock global que serializa TODAS las operaciones GET/SET/DEL de TODOS los stores/tenants, sin sharding ni partición por store. El trait `CacheBackend` es síncrono y se llama dentro del handler async sin `spawn_blocking`, con riesgo de saturar el pool de hilos de tokio bajo alta concurrencia. Esto ya estaba documentado sin profundizar en DEC-0037 ("el cache-server es punto único de fallo para operaciones de cache") y en DEC-0036 (el socket añade ~17ms de overhead vs ~26µs directo, medido con 1000 lecturas concurrentes — factor ~650x). Mosquitto es además un broker único sin clustering (ya listado como backlog no priorizado en ROADMAP.md). Escalar CPU/RAM al proceso cache-server no elimina la serialización del mutex ni el hop IPC — este es un límite genuino, no una configuración corregible en minutos.
  3. **Single-writer-por-store (DEC-0001/DEC-0002) es una decisión consciente, no un defecto**: "un solo escritor por store por diseño... no se puede escalar horizontalmente la escritura dentro de un store" fue aceptado desde el día 1 a cambio de cero `SQLITE_BUSY`. N stores = N writers independientes y paralelos — esto favorece un modelo multi-tenant (1 tenant = 1 store = 1 writer aislado) siempre que el cache-server (hallazgo 2) no vuelva a centralizar lo que el diseño de writers ya había descentralizado.
  4. **Veredicto de viabilidad**: técnicamente sólido para el núcleo (outbox transaccional, idempotencia, escritura serializada — validados con tests de crash y 0 errores en carga real, sin necesidad de rediseño). Parcialmente listo para producción (el instalador funciona end-to-end en Debian real, pero sin backpressure ni alertas de outbox implementadas — Fase 5 del ROADMAP nunca se hizo — cualquier carga sostenida no trivial hoy genera backlog sin límite). No viable *todavía* para alto volumen multi-tenant sin cerrar el hallazgo 1 (barato) y decidir explícitamente qué hacer con el hallazgo 2 (requiere diseño, no es un one-liner).
- Consecuencias: (+) Se distingue con evidencia de código (archivo+línea) qué es un bug barato de arreglar (publicador de outbox) y qué es una decisión de diseño no probada a escala (cache-server) — evita gastar esfuerzo corrigiendo el síntoma equivocado. (+) Backlog priorizado por severidad×esfuerzo queda registrado en `TODO.md`/plan de sesión para la siguiente fase: P0 fix del publicador, P1 backpressure real, P2 decisión de diseño sobre el cache-server, P3 load test real sin overhead de bash (`hey`/`vegeta`/cliente async, no el harness de curl secuencial en subshells), P4 el resto del backlog ya conocido (sharding interno de un store, clustering de Mosquitto, métricas de proyector/writer). (-) Ninguno de los 2 hallazgos se corrigió en esta tarea — es análisis, no implementación; el proyecto sigue sin backpressure y sin sharding de cache-server hasta que se ejecute el backlog. (-) No hay un número de throughput real de techo del servidor todavía — los benchmarks previos (DEC-0036) miden el cache-server aislado (1000 lecturas concurrentes), no el pipeline completo de escritura bajo carga sostenida con un cliente sin overhead de proceso-por-request.
- Reemplaza: none

### DEC-0044 — Fix del publicador de outbox (P0 de DEC-0043): batch UPDATE + publicación concurrente, backlog eliminado

- Fecha: 2026-08-09
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0008`
- Contexto: DEC-0043 identificó la causa raíz exacta del backlog de outbox observado en DEC-0042 (4762/13460 eventos sin publicar, ~35%): `event_publisher.rs` drenaba el outbox con un `tokio::interval` de 1000ms hardcodeado, publicando máximo 100 eventos por tick (también hardcodeado), de forma secuencial (`.await` por evento con QoS 1), y abriendo una conexión SQLite nueva por cada fila publicada solo para actualizar `published_at`. El backlog priorizado de DEC-0043 marcó esto como P0 — bajo esfuerzo, alto impacto, sin tocar el diseño de single-writer-por-store.
- Decisión: 1. `EventPublisher::publish_unpublished` (`crates/ixmati-writer/src/event_publisher.rs`) ahora hace fetch del batch con una sola conexión SQLite, publica todos los eventos concurrentemente vía `tokio::task::JoinSet` (clonando `AsyncClient`, que es `Clone` en rumqttc), y al final hace un único `Outbox::mark_published_batch` (`UPDATE _outbox SET published_at = datetime('now') WHERE id IN (...)`, nuevo en `outbox.rs`) para todos los eventos publicados con éxito, en vez de una conexión + UPDATE por fila. 2. `main.rs`: el intervalo de publicación (antes 1000ms hardcodeado) y el límite por tick (antes 100 hardcodeado) ahora son env vars `PUBLISH_INTERVAL_MS` (default 200) y `PUBLISH_BATCH_LIMIT` (default 500). 3. Se agregaron 2 tests unitarios para `mark_published_batch` (marca solo los ids dados, no-op con lista vacía).
- Verificación real (no solo "compila"): se reconstruyeron los binarios amd64, se reinstaló el stack en el mismo contenedor Debian de siempre, y se corrió el mismo load test concurrente de DEC-0042/0043 (20 workers, 30s) antes y después del fix. Antes: 4762/13460 eventos sin publicar (~35%), confirmado por gauge y por consulta SQL directa. Después: con carga equivalente (12753 writes, 425 ops/s reales), **0/12755 eventos sin publicar** — confirmado no solo por la métrica `ixmati_outbox_size` (que dejó de reportar la serie al llegar a cero, ya que la query `GROUP BY` no devuelve filas para un store sin backlog — comportamiento correcto pero visualmente distinto de "reportar 0 explícito", detalle menor pendiente de pulir si se usa para dashboards) sino por `SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL` ejecutado directamente vía python3/sqlite3 dentro del contenedor. Los 5 servicios systemd permanecieron activos durante y después de la carga en ambas corridas.
- Consecuencias: (+) El P0 del backlog de viabilidad (DEC-0043) está cerrado — el motor ya no acumula backlog sin límite bajo la carga probada (425-448 ops/s sostenidos por 30s). (+) La publicación concurrente y el batch UPDATE no requirieron cambiar el modelo de single-writer-por-store ni el schema del outbox. (-) No se probó con cargas sostenidas más largas (varios minutos) ni con múltiples stores concurrentes — el "backlog 0" se verificó para una corrida de 30s en un solo store; a mayor volumen sostenido podría reaparecer con otro techo, ahora presumiblemente mucho más alto que ~100 eventos/s pero no medido con precisión (queda para `TASK-VAL-0009` backpressure y `TASK-VAL-0011` load test real). (-) El gauge `OUTBOX_SIZE` no emite explícitamente el valor 0 cuando el backlog se vacía (la query solo agrupa stores CON backlog) — funcionalmente correcto pero un dashboard de Grafana vería "sin datos" en vez de una línea en 0, lo cual puede confundirse con "el proceso de self_monitor dejó de correr"; no corregido en esta tarea. (-) Persisten P1 (backpressure), P2 (decisión sobre cache-server) y P3 (load test real) del backlog de DEC-0043, sin tocar.
- Reemplaza: none

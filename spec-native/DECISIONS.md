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

### DEC-0008 — Confirmación durable de escritura: `accepted` como alias de `committed`

- **Fecha**: 2026-07-29
- **Estado**: `accepted`
- **Enmienda**: 2026-07-29 — el ack sync está acotado a 1 store; **invariante: un comando toca exactamente 1 store**
- **Relacionado con specs**: `SPEC-WRITE-0001`
- **Relacionado con tareas**: `TASK-WRITE-0009`, `TASK-WRITE-0010`
- **Contexto**: el contrato implementado debe distinguir una escritura durable confirmada de una solicitud que todavía está en proceso. La implementación actual no tiene un modo async de ACK inmediato: tanto `accepted` como `committed` esperan confirmación de `_idempotency` o devuelven `202 PENDING` al agotar el timeout.
- **Decisión**:
  1. El campo `ack_mode` acepta `accepted` y `committed` por compatibilidad, pero ambos tienen semántica durable.
  2. La API devuelve `200` sólo después de confirmar el commit en `_idempotency`.
  3. Si el commit no puede confirmarse dentro de `WRITE_COMMITTED_TIMEOUT_MS`, devuelve `202 PENDING`; el backend consulta `GET /writes/{store}/{idempotency_key}`.
  4. La correlación usa `(store, idempotency_key)`.
  5. **Invariante**: un comando apunta a 1 store. Si una operación de negocio requiere tocar 2 stores, la aplicación debe emitir 2 comandos separados y coordinar (saga fuera del motor).
  6. Un modo async de ACK inmediato requerirá una decisión y contrato separados; no se infiere de `accepted`.
- **Consecuencias**:
  - (+) No se confunde aceptación en memoria con durabilidad SQLite.
  - (+) Read-your-writes garantizado cuando la API devuelve `200`.
  - (+) Los benchmarks de `accepted` y `committed` pueden compararse porque comparten frontera de éxito.
  - (+) Sin transacciones distribuidas en el motor.
  - (-) No existe actualmente una opción de baja latencia con consistencia eventual.
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

### DEC-0045 — P1 (backpressure real por outbox) y P2 (fin del Mutex redundante en cache-server) del backlog de DEC-0043

- Fecha: 2026-08-10
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0009`, `TASK-VAL-0010`
- Contexto: tras cerrar P0 (DEC-0044), quedaban abiertos P1 (backpressure real usando `OUTBOX_SIZE`) y P2 (decisión de diseño sobre el cache-server centralizado, identificado en DEC-0043 como el techo arquitectónico real para multi-tenant de alto volumen).
- Decisión — P1: nuevo módulo `crates/ixmati-api/src/backpressure.rs` (`OutboxBacklog`) cachea el último `OUTBOX_SIZE` conocido por store, poblado por `self_monitor.rs` en cada poll de 5s. `write_handler` (`rest.rs`) rechaza con `429` + nueva variante `Error::OutboxBacklog` (`ixmati-core`) cuando el backlog de un store supera `OUTBOX_BACKPRESSURE_THRESHOLD` (env var, default 5000, `<=0` deshabilita). Al implementar esto se encontró y corrigió un bug real preexistente en `self_monitor.rs`: `collect_outbox_metrics` usaba `GROUP BY store` sobre filas sin publicar, así que un store que llegaba a backlog 0 simplemente desaparecía del resultado — ni la métrica `OUTBOX_SIZE` ni el backlog cacheado para backpressure se actualizaban a 0, quedando stale con el último valor distinto de cero para siempre. `OutboxBacklog::reset_missing()` corrige esto poniendo en 0 explícitamente cualquier store previamente conocido que no aparezca en el resultado del poll actual.
- Decisión — P2: se investigó si el `Mutex<Database>` de `RedbCacheBackend` era necesario antes de decidir entre sharding, `spawn_blocking`, o aceptar el límite (las 3 opciones que DEC-0043 había dejado abiertas). Resultado: el `Mutex` era completamente redundante. El código fuente de redb 4.1.0 (`db.rs`) documenta que `Database::begin_write(&self)` ya serializa internamente vía su propio `transaction_tracker` ("only a single write may be in progress at a time... this function will block until it completes"), y que `Database::begin_read(&self)` soporta lecturas concurrentes reales sin lock externo — `Database` es `Send + Sync` de fábrica (confirmado empíricamente: `cargo check` no habría compilado si no lo fuera, dado que `CacheBackend` requiere `Send + Sync`). El `Mutex` forzaba a que TODAS las lecturas de TODOS los stores/tenants formaran cola entre sí y con las escrituras, cuando redb ya las permite en paralelo — esto era el mecanismo concreto detrás del ~17ms de overhead medido en DEC-0036 bajo 1000 lecturas concurrentes. Se eliminó el `Mutex`; `RedbCacheBackend` sostiene `Database` directamente. Adicionalmente, `cache_server.rs` despacha cada operación (GET/SET/DEL/DEL_PREFIX/FLUSH) a `tokio::task::spawn_blocking`, para que una transacción larga no bloquee los hilos del executor async que atienden el resto de las conexiones del socket Unix (el segundo problema que DEC-0043 había señalado: el trait `CacheBackend` es síncrono y se llamaba directo dentro del handler async).
- Verificación: 6 tests nuevos en `backpressure.rs` (umbral, store desconocido, deshabilitado, reset de drenados, idempotencia del reset). `cargo test --workspace --lib` en verde. Validado en Debian real de punta a punta: (a) load test estándar tras el cambio de redb — 13382 writes, 0 errores, comportamiento idéntico al de antes del fix (confirma que remover el `Mutex` no rompió correctness); (b) prueba controlada de backpressure — con el writer detenido y 50 filas sintéticas insertadas directo en `_outbox` (umbral=20 configurado vía override), un `POST /write` fue rechazado con `429 {"OutboxBacklog":{"depth":50,"threshold":20}}` y log estructurado; al reiniciar el writer, `OUTBOX_SIZE` volvió a 0 explícito y el siguiente write fue aceptado automáticamente sin reiniciar el API, con `WRITE_ERRORS{error_type="outbox_backlog"}` contabilizado correctamente.
- Consecuencias: (+) P1 cierra el gap de producción-segura señalado en DEC-0043: el sistema ahora rechaza escrituras de forma explícita y recuperable en vez de acumular backlog sin límite indefinidamente. (+) P2 resultó ser una corrección mucho más simple y de menor riesgo que el sharding que DEC-0043 anticipaba como posible necesidad — no se tocó el modelo de un solo archivo/proceso Redb, solo se quitó un lock que nunca debió estar ahí. (+) El overhead de socket documentado en DEC-0036 (~17ms a 1000 lecturas concurrentes) debería reducirse sustancialmente para cargas de lectura-dominante, aunque no se re-corrió ese benchmark específico en esta tarea (queda como validación pendiente si se quiere un número actualizado). (-) El cache-server sigue siendo un solo proceso con un solo archivo Redb para todos los stores/tenants — esto mitiga la serialización *dentro* del proceso, no lo convierte en multi-tenant con aislamiento de fallos; si un store corrompe o satura el archivo Redb, sigue afectando a todos. Sharding real (archivos separados o particionamiento) sigue sin implementarse. (-) El umbral de backpressure (default 5000) es una elección arbitraria sin datos de producción reales que la respalden — debería revisarse una vez que P3 (load test real) dé un número de throughput sostenido confiable.
- Reemplaza: none

### DEC-0046 — P3: load test real con wrk revela techo de aceptación ~44k req/s, pero expone contención de CPU y el rate-limiter default como nuevos hallazgos

- Fecha: 2026-08-10
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0011`, `TASK-VAL-0013`, `TASK-VAL-0014`
- Contexto: DEC-0042/0043 dejaron establecido que el load test de `test_stack_validation.sh` (curl secuencial/paralelo en subshells de bash) mide el techo del harness, no del servidor — el propio proceso `curl` sin keep-alive y la contención entre procesos de bash dominan la latencia observada. P3 del backlog pedía un cliente de carga real sin ese overhead.
- Decisión: se usó `wrk` (disponible vía `apt-get install wrk` en Debian trixie, un solo proceso epoll sin overhead de fork/exec por request) con un script Lua nuevo (`helpers/wrk/write.lua`) contra `POST /write`, con `idempotency_key` únicos por request (semilla aleatoria por thread vía `math.randomseed`).
- Resultado — primer intento, con la config default: a la concurrencia que `wrk` puede generar (miles de req/s), el rate-limiter existente (`MAX_WRITES_PER_WINDOW=1000` escrituras/s por store en `throttle.rs`) rechazó el 99% de las requests con `429 QueueFull` — 462504 requests en 5s, 457275 no-2xx. Esto en sí mismo es un hallazgo real y no documentado antes con este nivel de precisión: **el techo de throughput sostenido en una instalación con la configuración default no es la capacidad del servidor, es el rate-limiter**, fijo en 1000/s/store, sin que el instalador lo exponga ni lo documente como parámetro operativo.
- Resultado — con `MAX_WRITES_PER_WINDOW` elevado temporalmente (para medir el código, no el rate-limiter): 3 minutos sostenidos, 7,974,686 requests, **44,281 req/s**, 0 errores HTTP (92 timeouts de socket, ~0.001%), p50=0.89ms, p99=34.75ms, RSS estable ~11MB durante toda la corrida, los 5 servicios activos al final. Una segunda corrida de 60s con claves verdaderamente únicas (la primera corrida reutilizó ids de thread entre invocaciones de `wrk`, causando colisiones de `idempotency_key` que sesgaron el conteo de outbox, no el de requests aceptadas) confirmó 45,531 req/s, 0 errores. Comparado con los 425-448 ops/s medidos con el harness de bash+curl (DEC-0042/0044): el techo real de aceptación de la API es **~100x mayor** que lo que el harness anterior podía generar.
- Caveat crítico, sin resolver en esta tarea: `wrk` corrió dentro del mismo contenedor Debian que los 5 servicios de Ixmati, sobre una VM de Podman con solo **2 vCPUs** asignadas — el generador de carga compite por los mismos cores que `ixmati-api`/`ixmati-writer`/`ixmati-cache-server`. Se observó vía `journalctl -u ixmati-writer` que bajo esta carga extrema el writer solo lograba comprometer ~100 batches/s a SQLite, notablemente por debajo de los ~425/s medidos en el test más liviano de DEC-0044 (donde `wrk` no estaba corriendo). No es posible con este montaje separar limpiamente "el writer tiene un techo real más bajo bajo carga extrema" de "el writer se quedó sin CPU porque `wrk` se la comió" — ambas explicaciones son plausibles y esta prueba no las distingue.
- Consecuencias: (+) Se tiene por primera vez un número de aceptación de la API medido sin el sesgo de proceso-por-request del harness anterior (44-45k req/s, 0 errores, memoria estable) — confirma que la capa HTTP/MQTT-publish de `ixmati-api` no es el cuello de botella para "alto volumen". (+) Se descubrió que el rate-limiter default (1000/s/store) es, en la práctica, el techo real de cualquier instalación sin tuning manual — una decisión de producto pendiente (`TASK-VAL-0013`), no cubierta por el trabajo previo de DEC-0043/44/45. (-) El número de throughput del *writer* bajo esta carga (~100 commits/s) NO debe citarse como el techo real del writer — está confundido con contención de CPU del propio test, y queda pendiente (`TASK-VAL-0014`) repetirlo con el generador de carga en una máquina/contenedor separado del target antes de sacar una conclusión sobre la capacidad real de escritura a SQLite. (-) `helpers/wrk/write.lua` documenta este caveat en su propio encabezado para que no se reutilice sin la salvedad.
- Reemplaza: none

### DEC-0047 — P4: instrumenta WRITE_BATCH_DURATION y PROJECTION_LAG en writer/projector; clustering de Mosquitto y sharding de store quedan como roadmap

- Fecha: 2026-08-10
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0012`
- Contexto: P4 de DEC-0043 agrupaba 3 ítems dispares: instrumentar 2 métricas que ya estaban definidas pero sin datos (`WRITE_BATCH_DURATION`, `PROJECTION_LAG`), clustering de Mosquitto, y sharding interno de un store. Los últimos 2 son features de infraestructura de alcance considerablemente mayor (semanas de trabajo, decisiones de topología no triviales) — se decidió no intentar implementarlos como parte de este backlog de viabilidad, y dejarlos explícitamente documentados como pendientes de roadmap en vez de simularlos o hacerlos a medias.
- Decisión: se instrumentaron las 2 métricas siguiendo el mismo patrón de `ixmati-api/src/metrics.rs` (OTel + exporter Prometheus + `encode_metrics()`), replicado en `crates/ixmati-writer/src/metrics.rs` y `crates/ixmati-projector/src/metrics.rs`. `WRITE_BATCH_DURATION` mide el tiempo real de `WriteEngine::process_batch` en `main.rs` de `ixmati-writer`. `PROJECTION_LAG` mide `ahora - occurred_at` del evento en el momento en que `ixmati-projector` termina de procesarlo — una elección deliberada de lag *temporal* en vez de lag *en cantidad de eventos*: el projector es puramente un consumidor de stream MQTT (`ixmati/evt/#`), sin visibilidad de un "total de eventos publicados" independiente de lo que ya recibió, así que no hay un offset autoritativo contra el cual medir backlog en unidades de eventos (a diferencia del `OUTBOX_SIZE` del writer, que sí tiene ese offset vía la tabla `_outbox`). El lag temporal es además la señal que el propio `ROADMAP.md` ya proponía como criterio de alerta ("lag de proyección > 1s"). Ambos endpoints `/metrics` son **opt-in vía `METRICS_PORT`**: si la env var no está seteada, no se abre ningún puerto (las métricas se siguen registrando en memoria igual, solo no hay HTTP que las exponga). Esto es intencional y no cosmético: `ixmati-writer` corre como unidad systemd template (`ixmati-writer@<store>`), una instancia por store — un puerto por defecto fijo colisionaría entre instancias del mismo host tan pronto hubiera 2+ stores. Un operador que quiera scrapear cada store por separado debe asignar un `METRICS_PORT` único por instancia vía drop-in de systemd.
- Verificación: 3 tests nuevos (2 en `ixmati-projector::metrics`, 1 en `ixmati-writer::metrics`). `cargo test --workspace --lib` en verde. Validado en Debian real: con `METRICS_PORT` configurado por override de systemd en `ixmati-writer@default` y `ixmati-projector`, tras generar 10 writes reales, `/metrics` del writer mostró `write_batch_duration_seconds_count=10` (mayoría de batches <1ms) y `/metrics` del projector mostró `projection_lag_seconds_count=9` (rango real 50-250ms) — datos genuinos de tráfico real, no ceros ni placeholders. Confirmado además que sin `METRICS_PORT` (el comportamiento default, sin cambios en el instalador) el resto del stack sigue funcionando exactamente igual — write/read normal probado tras el rebuild.
- Consecuencias: (+) Los 2 huecos de instrumentación que quedaban desde DEC-0043 (proyector/writer sin `/metrics`) están resueltos, y de forma verificada con datos reales, no solo "compila". (+) El diseño opt-in evita romper deployments multi-store existentes por colisión de puerto — un riesgo real que se identificó y evitó antes de escribir código, no después. (-) Clustering de Mosquitto y sharding interno de un store siguen sin resolver — el sistema sigue teniendo un broker MQTT único (sin HA) y un solo escritor/archivo SQLite por store (por diseño, DEC-0002) como límites conocidos y ya documentados en `ROADMAP.md`, no nuevos. (-) El instalador nativo (`installer.py`, unidades systemd) no expone todavía `METRICS_PORT` como opción configurable de forma visible — un operador tiene que saber que existe y agregarlo manualmente vía drop-in; no se integró al flujo de instalación estándar en esta tarea.
- Reemplaza: none

### DEC-0048 — TASK-VAL-0014: load test limpio revela pérdida silenciosa de escrituras bajo carga sostenida — "ACCEPTED" no garantiza durabilidad

- Fecha: 2026-08-10
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0014`
- **Severidad: CRÍTICA.** Este hallazgo revisa a la baja el veredicto de "técnicamente sólido" de DEC-0043 — el motor de escritura, bajo carga sostenida real, puede reconocer (`ACCEPTED`, HTTP 200) escrituras que nunca llegan a persistirse, sin que el cliente tenga forma de saberlo salvo consultando `GET /writes/{store}/{idempotency_key}` explícitamente (que nada obliga a hacer, y que "ack_mode: committed" sugiere — engañosamente — que no hace falta).
- Contexto: `TASK-VAL-0014` (pendiente desde DEC-0046) pedía repetir el load test de `wrk` desde fuera del contenedor target, para separar "capacidad del servidor" de "contención de CPU con el generador de carga" (la VM de Podman solo tiene 2 vCPUs, compartidas entre `wrk` y los 5 servicios en el intento anterior). Se instaló `wrk` en el host macOS (10 cores) vía Homebrew, se publicó el puerto 30000 del contenedor (`podman run -p 30000:30000 ...`), y se corrió una carga sostenida de 3 minutos contra `POST /write` con `MAX_WRITES_PER_WINDOW` elevado (para medir el código, no el rate-limiter).
- Resultado — capa HTTP: 3,232,239 requests en 3 minutos, **17,960 req/s** sostenidos, **0 errores HTTP** (54 timeouts de socket, ~0.0017%), p50=2.36ms, p99=720.66ms. A primera vista, un resultado limpio y mejor que el de DEC-0046 (sin la confusión de CPU compartida con el load generator).
- Resultado — capa de durabilidad, la parte crítica: de esas 3,232,239 escrituras "ACCEPTED", solo **27,400 (0.85%)** llegaron a comprometerse en `_outbox` — confirmado contando `committed` + `duplicates` sumados de TODOS los logs `"batch processed"` del writer durante la corrida completa (`duplicates=0` en la suma total, descartando de plano que fuera una colisión de `idempotency_key` del script de carga, como sí había pasado — y quedado documentado — en una corrida anterior de DEC-0046). El **99.15% restante de las escrituras que la API reconoció como `ACCEPTED` nunca se procesaron.**
- Causa raíz, confirmada en 3 niveles de evidencia independientes:
  1. **Los logs muestran el mecanismo exacto**: `ixmati-writer` fue **matado por el OOM killer del kernel dos veces** durante la corrida (`"A process of this unit has been killed by the OOM killer"`, confirmado por `systemctl status` mostrando `restart counter is at 2`), precedido en ambos casos por errores repetidos `"database is locked"` (en `event_publisher`, sin `busy_timeout` configurado — ver DEC-0043) y luego `"MQTT publisher eventloop error", "IO: Broken pipe"` / `"Connection closed by peer abruptly"` en el consumer.
  2. **El código fuente de `rumqttc` (`client.rs:69-89`) confirma por qué esto pierde datos silenciosamente**: `AsyncClient::publish()` hace `self.request_tx.send_async(publish).await?` y retorna `Ok(())` — esto solo confirma que el mensaje se encoló en un canal interno, **no que se entregó al broker**. La entrega real ocurre en una tarea separada (`eventloop.poll()`); si esa tarea encuentra un error de conexión (como los "Broken pipe" observados) mientras hay mensajes en tránsito, esos mensajes se pierden sin que el código que llamó a `publish().await` se entere — ya había retornado `Ok(())` antes.
  3. **`$SYS/broker/load/publish/dropped/*min` de Mosquitto muestra 0.00 en todas las ventanas** — descartando que el broker mismo esté descartando mensajes por cola llena (`max_queued_messages`). La pérdida ocurre *antes* de que el broker reciba el mensaje, en el cliente MQTT del proceso que publica (la API, y separadamente el propio `event_publisher` del writer), no en Mosquitto.
  4. **Hallazgo secundario, relacionado**: `ack_mode: "committed"` (`rest.rs`, línea ~315) **no espera ni verifica nada distinto de `"accepted"`** — ambos modos hacen exactamente el mismo `publish().await` y retornan de inmediato; la única diferencia es el texto del mensaje (`"published, waiting commit"` vs `"published"`). El nombre "committed" invita activamente a asumir una garantía que el código no provee.
- Consecuencias: (-) **El motor de escritura, bajo la carga que este mismo backlog de trabajo demostró que la capa HTTP puede aceptar (decenas de miles de req/s), pierde de forma silenciosa la inmensa mayoría de las escrituras** — esto contradice la garantía de durabilidad que es la razón de ser declarada del proyecto (outbox transaccional, at-least-once). El veredicto "técnicamente sólido" de DEC-0043 debe leerse ahora con esta salvedad: sólido *en el camino feliz y a las cargas ya probadas en DEC-0044/0045 (≤450 ops/s reales)*, no bajo la carga que la propia API puede aceptar sin rechazar. (-) El backpressure de P1 (DEC-0045) protege el path HTTP→outbox (rechaza si el outbox ya tiene backlog visible), pero **no protege contra el escenario documentado aquí**, que ocurre *antes* de que cualquier dato llegue al outbox — es un fallo en la capa de transporte MQTT del propio proceso, invisible a esa métrica. (+) Se descartó definitivamente la hipótesis alternativa (colisión de `idempotency_key` en el script de `wrk`) con evidencia directa de los propios contadores del writer (`duplicates=0`), evitando atribuir esto a un artefacto de test cuando es un hallazgo real de producto. (+) La causa raíz está identificada con precisión de línea de código (`rumqttc::AsyncClient::publish`, `client.rs:87`) y no es especulativa.
- No corregido en esta tarea (queda para las siguientes, ver TODO.md `TASK-VAL-0015..0017`): (1) el `ack_mode: "committed"` debería esperar confirmación real (polling interno a `/writes/{store}/{idempotency_key}` antes de responder, o rediseño), o como mínimo dejar de llamarse "committed" si no lo garantiza; (2) el cliente MQTT de la API y del writer deberían propagar errores del `eventloop` de vuelta a quien publicó, o el writer debería tener un límite de memoria/backpressure real en su consumo de MQTT en vez de poder ser OOM-killed; (3) `event_publisher` sigue sin `busy_timeout` en sus conexiones SQLite (ya señalado en DEC-0043, nunca corregido), causa directa de los errores "database is locked" que precedieron el OOM en ambas ocurrencias.
- Reemplaza: none (amplía/matiza el veredicto de DEC-0043, no lo reemplaza formalmente)

### DEC-0049 — Cierre del backlog de DEC-0048: busy_timeout, canal acotado con acks manuales, y ack_mode=committed real

- Fecha: 2026-08-10
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0015`, `TASK-VAL-0016`, `TASK-VAL-0017`
- Contexto: DEC-0048 encontró que, bajo carga sostenida real, el sistema podía reconocer (`ACCEPTED`, HTTP 200) escrituras que nunca se persistían — el writer murió por OOM-kill 2 veces y solo el 0.85% de 3.2M escrituras "aceptadas" llegó a `_outbox`. Se identificaron 3 causas concretas y se implementaron en orden de prioridad: primero lo que previene el crash, después lo que restaura la confianza del `ack_mode`.
- Decisión — `TASK-VAL-0017` (busy_timeout): nuevo módulo `crates/ixmati-writer/src/db.rs` (`open_with_pragmas`) centraliza `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;` — aplicado en los 4 sitios donde `ixmati-writer` abría conexiones SQLite sin ese pragma (2 en `main.rs`, 2 en `event_publisher.rs`), ninguno lo tenía antes.
- Decisión — `TASK-VAL-0016` (memoria acotada): `crates/ixmati-writer/src/consumer.rs` — `mpsc::unbounded_channel()` (causa directa del crecimiento de memoria sin límite) reemplazado por `mpsc::channel(capacity)` (`CONSUMER_CHANNEL_CAPACITY`, default 5000). `MqttOptions::set_manual_acks(true)`: los mensajes entrantes ya no se confirman automáticamente al broker — solo se ackean (`client.ack(&publish).await`) los que efectivamente entraron al canal acotado (`try_send` exitoso). Si el canal está lleno, el mensaje queda sin ackear a propósito y Mosquitto lo redistribuye según QoS1, trasladando la backpressure al broker (que ya tiene su propio límite documentado, `max_queued_messages`) en vez de acumularse sin límite en RAM del writer. Mensajes que fallan deserialización se ackean igual (reintentar no los arregla — evita un loop de redelivery infinito de "mensajes envenenados"). Métrica nueva `CONSUMER_QUEUE_DEPTH` en `metrics.rs`.
- Decisión — `TASK-VAL-0015` (`ack_mode: "committed"` real): `write_handler` (`rest.rs`) ahora hace polling real contra `_idempotency` (reutilizando `StatusQuery::query`, la misma consulta que ya usaba `GET /writes/{store}/{idempotency_key}`) antes de responder cuando `ack_mode == "committed"`. Si confirma `Applied` dentro de `WRITE_COMMITTED_TIMEOUT_MS` (default 2000ms, poll cada 30ms) responde `200` con `status: "APPLIED"` y los datos reales del commit. Si vence el timeout, responde `202` con `status: "PENDING"` — nunca `"ACCEPTED"` a secas. Si la API no tiene `SQLITE_PATH` configurado (instancias solo-cache), `ack_mode: "committed"` se rechaza con `400` explícito en vez de degradar silenciosamente al comportamiento de `"accepted"`. Lógica extraída a `wait_for_commit()` (función pura, testeable sin MQTT).
- Verificación: `open_with_pragmas` (1 test), `try_send`/backpressure del canal acotado (2 tests, validan el mecanismo real sin necesitar un broker), `wait_for_commit` (3 tests: aplicado inmediato, timeout, commit tardío dentro del deadline) — `cargo test --workspace --lib` en verde. **Validado en Debian real repitiendo la misma carga de 3 minutos de DEC-0048 (wrk desde el host, puerto publicado)**: 5,743,227 requests, 31,903 req/s, **0 errores HTTP, 0 timeouts de socket** (antes: 54), p99=4.14ms (antes: 720.66ms). `ixmati-writer` **0 reinicios** durante toda la corrida (antes: 2 OOM-kills), memoria estable en 15.3MB pico 15.9MB (antes: crecimiento sin límite hasta el kill del kernel), **0 líneas "database is locked"** en los logs (antes: repetidas, justo antes de cada crash). Con `ack_mode: "committed"` bajo el mismo backlog activo, una escritura nueva respondió honestamente `202 PENDING` en vez de un falso `200 ACCEPTED`.
- Hallazgo adicional de esta validación, importante para no sobre-vender el resultado: de las 5.7M escrituras aceptadas, solo ~5400 llegaron a comprometerse en la ventana de 3 minutos — pero esta vez **no es pérdida silenciosa**: `$SYS/broker/messages/stored` mostró 100,093 mensajes en cola en Mosquitto (su propio límite `max_queued_messages`), y `$SYS/broker/publish/messages/dropped` mostró ~5.6M mensajes descartados **por Mosquitto, de forma visible y contable vía un contador estándar de operación MQTT** — no por un crash silencioso del proceso. El techo real de comandos/segundo que el writer puede comprometer a SQLite en este contenedor de 2 vCPUs sigue siendo bajo (decenas/s bajo esta carga extrema), pero ahora falla de forma segura, acotada y observable en vez de catastrófica e invisible.
- Consecuencias: (+) Los 3 síntomas exactos de DEC-0048 (crash del writer, "database is locked", "ACCEPTED" fantasma) están cerrados con evidencia directa de la misma carga que los produjo. (+) El sistema ahora degrada de forma segura bajo sobrecarga: cola acotada y observable en el broker en vez de OOM del proceso. (+) `ack_mode: "committed"` cumple ahora lo que su nombre promete. (-) El throughput sostenido real de comprometido-a-SQLite sigue siendo mucho menor que lo que la capa HTTP acepta — esto no es un bug, es una decisión de capacidad (single-writer-por-store, DEC-0002) que sigue necesitando lo que ya estaba en el backlog de DEC-0043 (P1 backpressure ya implementado en DEC-0045 debería usarse en combinación con esto — un operador que quiera evitar la cola de Mosquitto llena debería bajar `OUTBOX_BACKPRESSURE_THRESHOLD` y/o el rate-limiter, no confiar solo en que Mosquitto absorba el exceso). (-) No se investigó a fondo cuál es el techo real de comandos/s que el writer puede comprometer de forma sostenida en hardware con más CPU — sigue siendo territorio de `TASK-VAL-0011`/`TASK-VAL-0014` (mismo problema de fondo: el contenedor de prueba tiene solo 2 vCPUs).
- Reemplaza: none (cierra el seguimiento abierto por DEC-0048)

### DEC-0050 — Investigación del techo de throughput del writer: 8 hipótesis probadas, 2 bugs reales corregidos, veredicto de capacidad

- Fecha: 2026-08-10
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: DEC-0049 cerró el hallazgo crítico de DEC-0048 (pérdida silenciosa de escrituras) pero de paso confirmó que, bajo la carga sostenida usada para probarlo, el writer solo compromete ~30-40 comandos/s a SQLite — muy por debajo de lo que la capa HTTP de la API puede aceptar. Esta decisión documenta la investigación completa (8 hipótesis, cada una probada con datos reales, no supuestos) para determinar la causa y el techo real de capacidad.
- **Metodología**: cada hipótesis se probó reconstruyendo binarios reales, desplegando en un contenedor Debian (primero en una VM local de podman, después en una **máquina Debian remota real** — `192.168.3.175`, amd64 nativo, 8 cores, sin emulación — tras verificar que la VM local de podman solo emulaba amd64 sobre arm64) y corriendo cargas sostenidas con `wrk` (`helpers/wrk/write.lua`) o benchmarks aislados nuevos (`crates/ixmati-writer/examples/bench_disk.rs`, `bench_contention.rs`, `bench_channel_loop.rs`) que no dependen de MQTT/red.

**Hipótesis probadas (todas con resultado negativo o parcial, en orden cronológico):**

| # | Hipótesis | Método | Resultado |
|---|---|---|---|
| A | `fsync` por commit (`synchronous=FULL` por defecto, nunca se aplicó `NORMAL` pese a que DEC-0001 lo decidió el día 1) | `PRAGMA synchronous=NORMAL` agregado a `crates/ixmati-writer/src/db.rs` | Commits: 5400→5700 (30→31.67/s). **Sin cambio significativo** |
| F | `cache_sync.sync_batch` secuencial + `block_in_place`/`block_on` innecesario | Reescrito a async nativo + despacho concurrente vía `JoinSet` en `cache_sync.rs` | Commits: 5700→5400 (31.67→30/s). **Sin cambio** |
| C | CPU insuficiente (contenedor de 2 vCPUs) | VM de podman reconfigurada a 6 vCPUs | Commits: 30→29.44/s. **Sin cambio, CPU descartado** |
| — | Entorno emulado (amd64 sobre arm64 vía QEMU) | Migración a máquina Debian real remota (amd64 nativo, 8 cores) | Commits: 34.44/s. **Sin cambio significativo respecto a 2-6 vCPU emulados** |
| G | `tokio::time::sleep(50ms)` recreado en cada iteración del loop (anti-patrón conocido: registrar un timer nuevo por vuelta en vez de un `interval` persistente) | `tokio::time::interval` creado una sola vez, `main.rs` | Commits: 34.44→35.64/s. **Sin cambio significativo**, pero cambió la composición de batches (más, más chicos) |
| — | Contención de lock SQLite entre la conexión del writer y las conexiones ad-hoc de `event_publisher.rs` | `bench_contention.rs`: dos conexiones al mismo archivo compitiendo, aislado | **1000/1000 commits/s, sin degradación** — reveló en cambio que el *publicador* queda hambriento (solo 3 de ~50 escrituras esperadas), no el writer |
| — | Mecánica del loop (`select!`+canal+`Batcher`) en sí | `bench_channel_loop.rs`: mismo patrón exacto de `main.rs`, sin MQTT real | **1099.9 commits/s** — descarta el loop como causa |
| H | `WriteEngine::process_batch` síncrono llamado directo dentro de una `async fn`, bloqueando un hilo del runtime de tokio (mismo anti-patrón ya corregido antes en `cache_server.rs`, DEC-0045) | `tokio::task::spawn_blocking` + `Mutex::blocking_lock()` en `main.rs` | Commits: 35.64→40.11/s. **Mejora real de ~13%**, la más grande de las 8 |

- **Benchmarks aislados creados** (quedan en el repo, reusables): `bench.rs` (ya existía; SQLite en memoria, 15k-26k commits/s), `bench_disk.rs` (SQLite en disco real vía `open_with_pragmas`, sin red: 16,230 writes/s en Mac, 3,918 writes/s en el disco del contenedor — confirma que el disco del contenedor es ~4x más lento que un SSD local pero de ninguna manera el cuello de botella), `bench_contention.rs`, `bench_channel_loop.rs`.
- **Descomposición final con 3 métricas simultáneas** (`WRITE_BATCH_DURATION`, `CACHE_SYNC_DURATION` nueva, `BATCH_FILL_DURATION` nueva — todas en `crates/ixmati-writer/src/metrics.rs`), medidas en la misma corrida de 40s / 86 batches en la máquina Debian remota real:

| Componente | ms/batch | % del ciclo |
|---|---|---|
| SQLite (`WRITE_BATCH_DURATION`) | 90.4ms | 19.4% |
| cache-server (`CACHE_SYNC_DURATION`) | 100.9ms | 21.7% |
| Llenar el batch (`BATCH_FILL_DURATION`, 100 `recv()`) | 85.1ms | 18.3% |
| **Total explicado** | **276.4ms** | **59.4%** |
| Ciclo real medido | 465.1ms | 100% |
| **Sin explicar** | **188.7ms** | **40.6%** |

- **Veredicto de causa raíz**: se identificaron y corrigieron **2 bugs reales** (F: `block_in_place` innecesario; H: SQLite bloqueante sin `spawn_blocking` — ambos del mismo anti-patrón de "código síncrono corriendo directo en el runtime async" ya visto una vez antes en `cache_server.rs`/DEC-0045). Ninguna de las otras 6 hipótesis (fsync, CPU, hardware/emulación, timer por iteración, contención de lock, mecánica del canal) explica el techo. El 40.6% del ciclo que ninguna métrica de aplicación captura es, con alta probabilidad, overhead de scheduling del runtime de tokio bajo contención real entre el `eventloop` de MQTT, el pool de `spawn_blocking`, las hasta 100 tareas concurrentes de `cache_sync`, y el servidor HTTP de métricas — compitiendo todos por los mismos hilos, algo que por naturaleza vive *entre* las llamadas instrumentadas, no dentro de ellas, y que herramientas de perfiling a nivel de sistema (`perf`, `strace`) podrían aislar mejor que métricas de aplicación. Esa investigación queda fuera de esta tarea (ver TODO.md).
- **Veredicto de capacidad**: el techo sostenido medido, de forma consistente en 8 escenarios distintos (2 máquinas físicas, 3 configuraciones de CPU, con y sin cada uno de los fixes), es de **~30-40 comandos/s comprometidos a SQLite por store**, bajo una carga de estrés sintética con el rate-limiter deliberadamente deshabilitado (`MAX_WRITES_PER_WINDOW=1000000`). El rate-limiter *default* de producción (`MAX_WRITES_PER_WINDOW=1000/s/store`, ver TASK-VAL-0013) ya es **~25x mayor** que esta capacidad real — es decir, en una instalación con la configuración default, el rate-limiter jamás protege al sistema porque el verdadero cuello de botella (el compromiso a SQLite) está muy por debajo de donde el rate-limiter empieza a actuar.
- Consecuencias: (+) 2 bugs reales de rendimiento corregidos con mejora medible (spawn_blocking: +13%), sin regresiones (workspace en verde en cada paso). (+) 3 benchmarks aislados nuevos quedan en el repo como herramienta reutilizable para medir estos componentes sin necesitar contenedores/red. (+) 3 métricas nuevas (`CACHE_SYNC_DURATION`, `BATCH_FILL_DURATION`, más `WRITE_BATCH_DURATION` de antes) dan visibilidad de producción real sobre dónde se va el tiempo en cada batch. (-) El 40.6% del ciclo sin explicar significa que la causa raíz completa NO está cerrada — se corrigieron los bugs encontrados, pero el techo de ~30-40/s persiste después de corregirlos, y no hay certeza de que más instrumentación de aplicación lo revele (haría falta profiling de sistema). (-) `OUTBOX_BACKPRESSURE_THRESHOLD` (default 5000, DEC-0045) y el rate-limiter default (1000/s, TASK-VAL-0013) deberían recalibrarse a esta capacidad real medida — no se hizo en esta tarea.
- Reemplaza: none

### DEC-0051 — Hilo dedicado (`WriteActor`) revertido: mejora de 4x en ráfaga, pero cuelgue silencioso a los ~90 batches

- Fecha: 2026-08-10
- Estado: `accepted` (decisión: NO adoptar la implementación probada; queda como investigación documentada)
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: DEC-0050 dejó abierta la alternativa de "hilo dedicado" (en vez de `spawn_blocking` por llamada, Opción H) para el techo de throughput del writer, y "profiling a nivel de sistema" (`TASK-VAL-0019`) para explicar el 41% del ciclo sin atribuir. Se implementó el hilo dedicado (`TASK-VAL-0021`, `WriteActor`) y, al encontrar un cuelgue real bajo carga, se investigó con las herramientas de profiling disponibles en el contenedor (`TASK-VAL-0022`).
- Implementación probada: `crates/ixmati-writer/src/write_actor.rs` (nuevo) — un único hilo de sistema operativo, creado una vez al arrancar, dueño exclusivo de la conexión SQLite, recibiendo trabajo (`ProcessBatch`, `MarkPublished`) por un canal `std::sync::mpsc` y respondiendo por `oneshot`. `main.rs` reemplazó `Arc<Mutex<Connection>>` + `spawn_blocking` por este actor. Al descubrir que `event_publisher.rs` también abría conexiones SQLite propias sin aislar (mismo anti-patrón, nunca corregido hasta ahora), se extendió el actor para que `mark_published_batch` también pasara por el mismo hilo/conexión — ninguna otra conexión de escritura al archivo.
- Resultado — throughput: **158.35 commits/s** en una corrida de 40s (vs 40.11 commits/s de Opción H, `spawn_blocking`) — una mejora de **~4x**, la más grande de toda la investigación de DEC-0050.
- Resultado — estabilidad, el problema real: en corridas de 3 minutos, el writer **dejó de procesar batches nuevos después de ~90-95 batches** (consistente en 2 corridas independientes, con y sin la corrección de `event_publisher`), sin ningún error, panic, ni reinicio de systemd (`NRestarts=0` durante todo el cuelgue). `wrk` siguió recibiendo respuestas exitosas de la API sin errores durante todo el período — el cuelgue es invisible para el cliente, solo visible al comparar el conteo de `"batch processed"` en los logs del writer contra el tiempo transcurrido.
- Investigación del cuelgue (`TASK-VAL-0022`): `strace` no funcionó en el contenedor (`ptrace: Operation not permitted`, política de ptrace anidada del host — instalado correctamente pero bloqueado al adjuntarse). Se usó `/proc/<pid>/task/*/wchan` y `/proc/<pid>/task/*/status` como alternativa sin `ptrace`: **los 12 hilos del proceso estaban dormidos (`state=S`)**, ninguno bloqueado en una syscall de I/O activa — no es el deadlock de contención SQLite que se sospechaba inicialmente (el hilo del `WriteActor` estaba esperando en `rx.recv()`, es decir, ocioso, no atascado procesando). `$SYS/broker/clients/#` de Mosquitto mostró 4 clientes conectados, 0 desconectados, pero `messages/stored` en 100,093 (el tope del broker) — el consumidor MQTT del writer seguía "conectado" a nivel de socket pero dejó de recibir/procesar mensajes nuevos, sin ningún error en los logs de Mosquitto ni del writer. La causa raíz exacta **no se determinó** — es consistente con algún tipo de pérdida de wakeup entre el eventloop de `rumqttc` y el resto del pipeline, pero no se pudo confirmar con las herramientas disponibles en este entorno.
- Decisión: **se revirtió la implementación completa** (`write_actor.rs` eliminado, `main.rs`/`event_publisher.rs`/`lib.rs` vueltos al estado del commit `47e859d`). Un writer que deja de procesar silenciosamente, sin ningún error observable, es un riesgo de producción peor que uno simplemente más lento — `spawn_blocking` (Opción H) fue validado en múltiples corridas de 3 minutos sin ningún cuelgue, y esa sigue siendo la implementación en producción.
- Consecuencias: (+) Se documenta con evidencia real que el hilo dedicado, aunque conceptualmente más simple y con mejor throughput medido en ráfaga, introduce un modo de falla nuevo y grave no presente en la implementación con `spawn_blocking` — esto es información valiosa para no reintentar este camino sin resolver primero el cuelgue. (+) Se confirmó independientemente el bug de `event_publisher.rs` (conexiones SQLite sin aislar, mismo anti-patrón que `process_batch` tenía) — este hallazgo secundario sigue siendo válido y queda pendiente de corregir por separado, con `spawn_blocking` (no con un hilo dedicado compartido, dado el resultado de esta decisión). (-) La causa raíz del cuelgue queda sin diagnosticar — `TASK-VAL-0019`/`TASK-VAL-0022` (profiling de sistema) no llegó a una conclusión definitiva; herramientas más profundas (`tokio-console`, o un host sin restricciones de `ptrace` anidado) quedan como posible camino futuro si se retoma esta línea de investigación. (-) El código de `write_actor.rs` no se preservó en una rama separada — si se retoma, habría que reimplementarlo desde este documento (la lógica está descrita en detalle arriba).
- Reemplaza: none (amplía DEC-0050, no cambia su veredicto de capacidad — sigue siendo ~30-40 commits/s con la implementación actual en producción)

### DEC-0052 — Fix de `event_publisher.rs` (spawn_blocking) + propuesta de motor de escritura síncrono sin tokio

- Fecha: 2026-08-10
- Estado: fix `accepted` e implementado; propuesta de motor síncrono `PROPUESTA — no implementada`
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: el usuario pidió reexaminar la investigación de DEC-0050/0051 con pruebas unitarias puras (sin necesitar Debian real) para encontrar la pieza que pudiera faltar, y una propuesta de arquitectura alternativa completa para el motor de escritura. Lectura completa del código (2 agentes de exploración, solo lectura) confirmó el bug ya anotado como abierto en DEC-0051: `event_publisher.rs` sigue llamando SQLite síncrono directo dentro de `async fn`, sin `spawn_blocking`.

**Hallazgo A — el hilo que faltaba, con mecanismo confirmado:**
`crates/ixmati-writer/src/event_publisher.rs::publish_unpublished` (ejecutada cada `PUBLISH_INTERVAL_MS`=200ms) hacía 4 llamadas SQLite síncronas directo dentro de una `async fn`, sin `spawn_blocking`: apertura de conexión + `Outbox::fetch_unpublished` (hasta 500 filas con `payload BLOB`), y una segunda apertura + `Outbox::mark_published_batch` (`UPDATE ... WHERE id IN (...)` con hasta 500 placeholders, toma el lock de escritura de SQLite). `main.rs` usa `#[tokio::main]` sin `worker_threads` explícito → en el contenedor de producción (2 vCPUs) son exactamente 2 hilos worker de tokio, compartidos también por el loop principal, los eventloops MQTT del consumer y del publisher, y el servidor de métricas. Bloquear uno de esos 2 hilos con I/O síncrono (potencialmente hasta `busy_timeout`=5000ms bajo contención) le quita hasta el 50% de la capacidad de cómputo del runtime al resto — candidato directo y medible al ~41% de ciclo por batch que DEC-0050 dejó sin explicar.

**Prueba unitaria del mecanismo** (nuevo módulo `event_publisher::tests::worker_starvation`, sin Debian ni carga real): dos tests `#[tokio::test(flavor = "multi_thread", worker_threads = 1)]` con una tarea "ticker" (incrementa un contador cada 1ms) corriendo en paralelo con una tarea bloqueante de 300ms lanzada vía `tokio::spawn` (mismo patrón que `publish_loop` en producción). Con la llamada bloqueante ejecutada directo (sin `spawn_blocking`), el ticker avanza ≤3 ticks durante los 300ms (prácticamente congelado). Con la misma llamada envuelta en `spawn_blocking`, el ticker avanza >100 ticks (progreso normal). `worker_threads = 1` se usó a propósito para aislar el mecanismo de forma determinística — reproduce el instante en producción en que ambos hilos worker ya están ocupados con otras tareas del pipeline y la llamada síncrona tiene el runtime para sí sola. Nota técnica encontrada al construir el test: llamar la función bloqueante directo desde el cuerpo de un `#[tokio::test]` no sirve para reproducir el bug — ese cuerpo corre en el hilo que llama a `block_on`, que es distinto del pool de workers usado por tareas `tokio::spawn`, incluso con `worker_threads=1`; hace falta lanzarla con `tokio::spawn` (como realmente hace `publish_loop`) para que compita por el mismo pool que el resto de las tareas.
- Archivo: `crates/ixmati-writer/src/event_publisher.rs` (tests `sync_call_without_spawn_blocking_freezes_other_tasks` / `sync_call_wrapped_in_spawn_blocking_does_not_freeze_other_tasks`).

**Fix aplicado** (`TASK-VAL-0019`, bajo riesgo): las 4 llamadas de `publish_unpublished` se envolvieron en 2 `tokio::task::spawn_blocking` (uno para apertura+fetch, otro para apertura+mark_published_batch), mismo patrón que `process_batch` (Opción H, DEC-0050). `cargo test --workspace --lib` en verde (29/29 en `ixmati-writer`, sin regresiones en el resto del workspace). Pendiente: revalidar en Debian real con la misma metodología de `wrk` de sesiones anteriores para medir cuánto del 41% no explicado cierra esto — no se hizo en esta sesión (sin acceso a la infraestructura remota en este momento).

**Hallazgo B — por qué este bug ya apareció dos veces:** mezclar tokio (diseñado para I/O no bloqueante) con SQLite (inherentemente síncrono, sin API async real) en el mismo runtime es frágil por construcción — cada nuevo call site necesita que alguien se acuerde de `spawn_blocking`, sin que el compilador lo fuerce. Ya pasó en `process_batch` (DEC-0050) y en `event_publisher.rs` (acá). El hilo dedicado de DEC-0051 (`WriteActor`) intentó resolverlo aislando la conexión SQLite, pero seguía comunicándose con el resto del sistema vía `tokio::sync::oneshot` esperado desde tareas async — la misma frontera frágil, en sentido inverso. Confirmado con el usuario que esta lectura explica el cuelgue silencioso de DEC-0051: el hilo de SO quedaba aislado, pero seguía "enganchado" al runtime async por el canal de respuesta.

**Propuesta (no implementada) — motor de escritura 100% síncrono, sin tokio:** el proceso `ixmati-writer` dejaría de usar tokio por completo. Un único hilo de SO, dueño exclusivo de la conexión SQLite, con un loop síncrono de `recv()` bloqueante sobre una cola acotada (`std::sync::mpsc`/`crossbeam-channel`, límite explícito, backpressure real al productor). El I/O externo (cliente MQTT, socket del cache-server) correría en sus propios hilos dedicados, cada uno con su propia cola acotada hacia/desde el hilo de SQLite, sin runtime async compartido entre ellos. `rumqttc` 0.25.1 (ya vendorizado) expone una API síncrona nativa (`rumqttc::Client`/`rumqttc::Connection`, `client.rs:251`/`:433`) — no haría falta una dependencia nueva. Esto haría consistente lo que el hilo dedicado de DEC-0051 ya intentaba: el hilo de escritura dejaría de ser una isla síncrona rodeada de canales async y pasaría a ser el diseño completo, con colas bloqueantes puras en ambas direcciones — eliminando la clase de bug de raíz (no hay `async fn` que "olvidarse" de envolver, porque no habría `async fn` en el camino de escritura) y evitando el modo de falla de `WriteActor` (ningún canal de tokio cruzaría la frontera).
- Pros: elimina la clase de bug recurrente de raíz; modelo mental más simple (un hilo, una cola, sin runtime compitiendo); backpressure end-to-end explícito y visible en cada cola.
- Contras: reescritura no trivial de `consumer.rs`, `event_publisher.rs` y `cache_sync.rs`; pierde el paralelismo fino de tokio para I/O concurrente (aunque DEC-0050 ya demostró que ese paralelismo no se traducía en throughput real, porque SQLite es de un solo escritor de todos modos); requiere revalidar todo el pipeline en Debian real desde cero.
- Riesgo de la propuesta en sí: ninguno — es una decisión pendiente, no código en producción.
- Consecuencias: (+) el fix de `event_publisher.rs` cierra un bug real y documentado con una prueba unitaria reproducible que no depende de infraestructura externa. (+) queda un diseño alternativo completo, con el mecanismo de su predecesor fallido (DEC-0051) explicado y su cliente MQTT síncrono ya verificado en el workspace, listo para arrancar como esfuerzo separado si se decide perseguirlo. (-) el fix no se validó todavía con carga real en Debian — el impacto exacto sobre el 41% no explicado de DEC-0050 sigue siendo una hipótesis fuerte, no un número confirmado. (-) la propuesta del motor sin tokio no se implementó — sigue siendo una decisión de arquitectura pendiente del usuario.
- Reemplaza: none (cierra el bug documentado-pero-no-corregido de DEC-0051; amplía DEC-0050/0051 con una propuesta de arquitectura, no cambia ningún veredicto previo)

### DEC-0053 — Implementación del motor de escritura síncrono sin tokio (propuesta de DEC-0052 llevada a código)

- Fecha: 2026-08-10
- Estado: `accepted` e implementado
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: el usuario pidió implementar la propuesta documentada en DEC-0052. Se reescribió `ixmati-writer` por completo para eliminar tokio del proceso.

**Cambios de código:**
- `crates/ixmati-cache/src/cache_client_sync.rs` (nuevo) — `SyncCacheClient`, mismo protocolo de texto que `CacheClient` (async) pero sobre `std::os::unix::net::UnixStream` bloqueante con `set_read_timeout`/`set_write_timeout`(5s) en vez de `tokio::time::timeout`. El wire format es idéntico — confirmado por exploración que el servidor (`cache_server.rs`) no depende de que el cliente use tokio, solo lee/escribe bytes sobre el socket.
- `crates/ixmati-writer/src/write_thread.rs` (nuevo, reemplaza conceptualmente al `write_actor.rs` revertido de DEC-0051, pero corrigiendo su defecto de diseño) — un hilo de SO dedicado, dueño exclusivo de la conexión SQLite, recibe `Job`s por `std::sync::mpsc::Sender/Receiver` y responde por un canal `std::sync::mpsc` creado por-llamada (patrón "oneshot vía mpsc"). **Sin ningún `tokio::sync::oneshot`** — a diferencia de `WriteActor`, el hilo llamador (el loop principal en `main.rs`) es ahora un hilo de SO normal, no una tarea de tokio, así que no hay ninguna tarea async bloqueada esperando la respuesta.
- `crates/ixmati-writer/src/consumer.rs` (reescrito) — `rumqttc::Client`/`Connection` (API síncrona) en vez de `AsyncClient`/`EventLoop`; el canal hacia el batcher es `std::sync::mpsc::sync_channel` (con `try_send`/backpressure real, mismo mecanismo que antes) en vez de `tokio::sync::mpsc`. La conexión MQTT corre en su propio hilo de SO dedicado.
- `crates/ixmati-writer/src/event_publisher.rs` (reescrito) — mismo patrón síncrono; ya no necesita `spawn_blocking` (el fix de DEC-0052) porque no hay ningún runtime al que proteger — el hilo dedicado que la corre no comparte nada con nadie más.
- `crates/ixmati-writer/src/cache_sync.rs` (reescrito) — usa `SyncCacheClient`, llamadas secuenciales en vez del `JoinSet` concurrente (que nunca lograba paralelismo real de I/O de todos modos, según el propio análisis de DEC-0050 — el `CacheClient` serializa todo sobre un único socket con mutex).
- `crates/ixmati-writer/src/metrics.rs` — el servidor HTTP `/metrics` (axum) sigue existiendo, pero ahora corre en su propio hilo con su propio `tokio::runtime::Builder::new_current_thread()` confinado — igual patrón que usa `rumqttc::Client` internamente. Este runtime nunca toca SQLite ni comparte primitivas con el hilo de escritura.
- `crates/ixmati-writer/src/batcher.rs` — `tokio::time::{Duration, Instant}` → `std::time::{Duration, Instant}` (cosmético, `tokio::time::Duration` ya era un re-export de `std::time::Duration`; `Instant` si era un wrapper propio de tokio, aunque no requería runtime para funcionar).
- `crates/ixmati-writer/src/main.rs` (reescrito) — `fn main()` normal, **sin `#[tokio::main]`**. El loop principal usa `std::sync::mpsc::Receiver::recv_timeout` en vez de `tokio::select!` para el mismo rol (esperar un comando o el vencimiento del intervalo de flush).
- Se eliminaron las 2 pruebas unitarias de `worker_starvation` de DEC-0052 (`event_publisher.rs`) — probaban un mecanismo (competencia por hilos worker de tokio) que ya no existe: no hay runtime tokio compartido del que algo pueda "morir de hambre".

**Verificación:**
- `cargo build --workspace` y `cargo test --workspace --lib` en verde — 8/8 crates, sin regresiones (mismo conteo de tests que antes, menos los 2 tests de starvation ya no aplicables, más 2 tests nuevos de `write_thread.rs` y 2 de `cache_client_sync.rs`).
- **Smoke test end-to-end real** (Mosquitto + `ixmati-cache-server` + `ixmati-writer`, los 3 binarios reales corriendo en local, sin mocks): se publicaron comandos por MQTT, se confirmó en los logs `"batch processed"` con `committed` correcto, se verificaron las filas reales en `payload_pedidos` (SQLite) y el valor correcto vía `GET` directo al socket del cache-server.
- **Prueba de carga sostenida de 70s** (publicación continua por MQTT, sin pausas): **3,030 batches procesados, 15,150 comandos comprometidos, 0 errores, 0 panics, outbox completamente drenado al final (0 filas sin publicar)**, progreso continuo verificado cada 10s (sin ningún signo del cuelgue silencioso de DEC-0051 — ahí el `WriteActor` se congelaba consistentemente después de ~90-95 batches; acá se pasaron 3,030 sin incidentes).
- No se corrió todavía en el contenedor Debian real de las sesiones anteriores (sin acceso a esa infraestructura en este momento) — el smoke test fue en macOS local con Mosquitto/cache-server/writer reales, no en el contenedor de producción de 2 vCPUs. Pendiente: repetir la validación de `wrk` de sesiones anteriores en Debian real para confirmar el throughput sostenido bajo la carga HTTP completa (no solo publicación MQTT directa) y medir si el 41% de ciclo sin explicar de DEC-0050 se cierra.
- Consecuencias: (+) resuelve de raíz la clase de bug de DEC-0050/0052 (código síncrono compitiendo con tokio) — no puede volver a ocurrir porque no hay runtime compartido en el que ocurra. (+) evita, con evidencia de una prueba de carga sostenida real, el modo de falla exacto que hizo revertir `WriteActor` en DEC-0051. (+) modelo de concurrencia más simple de razonar: hilos de SO + `std::sync::mpsc`, sin mezclar con async en ningún punto del camino de escritura. (-) no validado todavía en el contenedor Debian de producción ni bajo la carga HTTP completa de `wrk` — el smoke test valida corrección y ausencia de cuelgues, no el throughput final bajo las mismas condiciones que DEC-0049/0050. (-) `standby.rs`/`cache_responder.rs` (código muerto, no referenciado por `main.rs`) no se migraron a la API síncrona — si en el futuro se activan, necesitarían el mismo tratamiento.
- Reemplaza: none (implementa la propuesta de DEC-0052; no cambia ningún veredicto de capacidad anterior, que sigue pendiente de remedir en Debian real)

**Actualización — validado en el contenedor Debian real de producción** (mismo host amd64 remoto usado en toda la investigación de DEC-0050/0051, `debian:trixie-slim` con systemd real, `--privileged`):

- **Bug preexistente encontrado y corregido de paso**: `systemd/ixmati-api.service` nunca seteaba `SQLITE_PATH` — desde que DEC-0049/`TASK-VAL-0015` implementó la confirmación real de `ack_mode: committed` (que necesita `SQLITE_PATH` en la API para consultar `_idempotency`), el instalador nunca había vuelto a probarse con ese modo, y el propio `helpers/shell/test_installer_debian.sh` (que sí usa `ack_mode: committed` en su roundtrip) fallaba en el paso 4/7 con `"no database configured for reads"`. Se agregó `Environment=SQLITE_PATH=/var/lib/ixmati/stores/default.db` a la unidad. **No relacionado con el motor síncrono de esta decisión** — es un gap de la capa API, confirmado por `git log` que predata esta sesión.
- **Instalador completo (7/7 pasos) en verde** con el fix aplicado: instalación limpia, los 5 servicios `active` (incluyendo `ixmati-writer@default` corriendo con el nuevo binario sin tokio), roundtrip `ack_mode: committed` → `{"status":"APPLIED",...}` con commit real confirmado, reinstalación idempotente, desinstalación con purga limpia.
- **Carga sostenida de 3 minutos con `wrk`** (`-t4 -c50 -d180s`, mismo script y metodología de DEC-0046/0049, corrido desde el Mac contra el puerto publicado del contenedor remoto, rate-limiter de la API elevado a 1,000,000/s para medir el techo real del writer, no el del limitador): **464,474 requests aceptadas por la API, 0 errores de ningún tipo** (conexión, lectura, escritura, timeout, status), p50=17.93ms, p99=36.45ms.
- **Lado del writer**: 65 batches procesados, **6,500 comandos comprometidos a SQLite** (confirmado con `sqlite3` dentro del contenedor: 6,500 filas reales en `payload_default`), **0 reinicios** (`NRestarts=0`), **0 líneas de error/panic** en el journal, memoria estable (15.4MB RSS, sin crecimiento), outbox completamente drenado (0 filas sin publicar) — mismo patrón limpio que el smoke test local de 70s, ahora confirmado en el contenedor de producción real.
- **Hallazgo honesto — el throughput NO mejoró**: 6,500 comandos en 180s ≈ **36.1 commits/s**, dentro del mismo rango de ~30-40 commits/s medido en DEC-0049/0050 con la implementación anterior (`spawn_blocking`). El motor síncrono resuelve la fragilidad arquitectónica (elimina la clase de bug de mezclar tokio con SQLite, evita el cuelgue de `WriteActor`) pero **no mueve la aguja del techo de comandos/s** — consistente con la hipótesis original de DEC-0050 de que el costo dominante es el commit/fsync de SQLite en sí (I/O), no el scheduling del runtime. El 41% de ciclo sin explicar de DEC-0050 sigue sin una explicación definitiva; esta implementación no lo cerró.
- **Observación fuera de alcance de esta decisión** (comportamiento heredado sin cambios de `consumer.rs`, ya presente en la versión async): bajo esta sobrecarga deliberada de ~25x la capacidad real (464K aceptadas contra ~36/s de capacidad), `$SYS/broker/messages/stored` del propio Mosquitto quedó en su tope (100,093, cerca del `max_queued_messages` default) con solo 5 mensajes reportados como `dropped` — la aritmética exacta de qué pasó con el resto del backlog bajo sobrecarga extrema no se investigó a fondo acá, coincide con el trabajo ya pendiente en `TASK-VAL-0020` (recalibrar `OUTBOX_BACKPRESSURE_THRESHOLD`/rate-limiter a la capacidad real).
- Consecuencias adicionales: (+) el motor síncrono queda validado end-to-end en las mismas condiciones (contenedor, host, metodología) que toda la investigación previa — no es solo "compila" ni "funciona en macOS local". (+) se encontró y corrigió un bug preexistente no relacionado que bloqueaba cualquier prueba futura con `ack_mode: committed`. (-) el objetivo original de mejorar/explicar el throughput sigue abierto — DEC-0053 es una mejora de robustez, no de capacidad; `TASK-VAL-0020` sigue siendo necesario y ahora con más urgencia dado que el gap entre lo que la API acepta y lo que el writer sostiene se confirmó una vez más a escala real.

### DEC-0054 — Recalibración del rate-limiter/backpressure a la capacidad real medida (TASK-VAL-0020)

- Fecha: 2026-08-10
- Estado: `accepted` e implementado
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: DEC-0053 confirmó (otra vez, a escala real en Debian) que el writer sostiene ~36 commits/s, contra un rate-limiter default de 1000/s (~25x mayor) y un `OUTBOX_BACKPRESSURE_THRESHOLD` de 5000. El usuario pidió cerrar `TASK-VAL-0020`: recalibrar ambos a la capacidad real.

**Hallazgo previo al cambio — `OUTBOX_BACKPRESSURE_THRESHOLD` es ciego al modo de falla que importa:** revisando `self_monitor.rs` se confirmó que `OutboxBacklog` mide `SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL` — filas **ya comprometidas a SQLite** esperando publicarse a MQTT. No mide la cola de comandos aceptados por la API pero aún sin comprometer (esa cola vive en Mosquitto/el canal acotado del consumidor MQTT del writer, un proceso distinto). En la carga de 3 minutos de DEC-0053 (464K aceptadas, throttle deshabilitado a propósito, `OUTBOX_BACKPRESSURE_THRESHOLD` en su default de 5000 sin tocar), **el backpressure del outbox nunca se disparó ni un solo 429** — porque `_outbox` se mantuvo casi vacío todo el tiempo (el publicador drena mucho más rápido de lo que el writer compromete). Bajar ese umbral por sí solo no habría cerrado el gap real; el mecanismo que sí puede actuar a tiempo es el rate-limiter (`throttle.rs`), que rechaza *antes* de que cualquier cosa se encole en cualquier lado.

**Cambios de código** (`crates/ixmati-api/src/rest.rs`):
- `MAX_WRITES_PER_WINDOW` default: `1000` → **`40`** (extraído a constante `DEFAULT_MAX_WRITES_PER_WINDOW`). Cerca del límite superior del rango medido (~30-40/s), con margen pequeño sobre el peor caso observado (36.1/s) en vez de ~25x por encima.
- `OUTBOX_BACKPRESSURE_THRESHOLD` default: `5000` → **`500`** (constante `DEFAULT_OUTBOX_BACKPRESSURE_THRESHOLD`). No cierra el hallazgo de arriba (sigue midiendo la cola equivocada para el caso "ingestión > capacidad de escritura"), pero sí mejora su utilidad para el modo de falla que sí puede detectar (el publicador de eventos atascado): a la tasa máxima de drenado, 500 se dispara en ~14s en vez de ~139s con el default anterior.
- Se deduplicó la lectura de `MAX_WRITES_PER_WINDOW`/`THROTTLE_WINDOW_SECS` (antes repetida en `AppState::new` y `AppState::with_db`) en 2 funciones helper.
- `systemd/ixmati-api.service`: se agregó un comentario documentando los 3 tunables y sus nuevos defaults, con líneas `Environment=` comentadas listas para descomentar — se evitó fijarlos explícitos para no duplicar el valor y arriesgar que la unidad quede desincronizada del binario si el default cambia en el futuro. Esto también cierra la pregunta de producto abierta en `TASK-VAL-0013` ("¿el default debería ser visible/documentado en el instalador?") — ahora lo está.
- Prueba nueva (`rest::tests::recalibrated_defaults_match_measured_writer_capacity`) que fija los 3 valores por constante en vez de por env var (evita mutar estado de proceso compartido entre tests en paralelo) — un cambio accidental del default se nota en el diff de una prueba, no solo en producción.

**Verificación — validado en el mismo contenedor Debian real usado en DEC-0053** (host amd64 remoto, systemd real):
- `cargo test --workspace --lib`: 8/8 crates en verde.
- Instalación limpia con los nuevos defaults **sin overrides** (a diferencia de DEC-0053, que deshabilitaba el throttle a propósito para medir el techo del writer).
- Carga de 1 minuto con `wrk -t4 -c50 -d60s` (mismo script `helpers/wrk/write.lua`): de 134,544 requests, **132,144 rechazadas con backpressure** (status non-2xx) — el throttle actuando desde el principio, no después de acumular un backlog invisible. Solo **2,400 aceptadas** (≈40/s × 60s, matemática exacta del nuevo límite).
- Lado del writer: **2,400 comandos comprometidos — exactamente igual a las 2,400 aceptadas.** Por primera vez en toda esta investigación, `aceptadas == comprometidas`: cero backlog oculto. 74 batches procesados, 0 reinicios (API y writer), 0 errores/panics.

- Consecuencias: (+) cierra el gap real: ahora la API rechaza activamente en vez de aceptar 25x más de lo que el writer puede sostener y dejar que el exceso se pierda en la opacidad de la cola de Mosquitto. (+) comportamiento verificado con datos reales, no solo el cambio de un número: `aceptadas == comprometidas` bajo la misma carga que antes generaba un gap de ~72x. (+) se documenta un hallazgo arquitectónico real y antes desconocido: `OutboxBacklog` mide la cola equivocada para el modo de falla "ingestión supera capacidad de escritura" — queda registrado para que nadie asuma que bajar ese umbral por sí solo resuelve ese caso. (-) el rate-limiter es ahora estrictamente por-store y por-ventana-de-1s; un store con tráfico legítimamente ráfaga por encima de 40/s en momentos puntuales (pero que en promedio el sistema podría absorber vía batching) ahora se rechaza más agresivamente que antes — trade-off consciente, dado que el mecanismo "más inteligente" (backpressure de outbox) resultó ciego a este caso. (-) no se cerró el hallazgo de "qué pasó exactamente con el backlog restante" de DEC-0053 — con el rate-limiter ahora actuando desde el inicio, ese escenario específico (throttle deshabilitado + backlog masivo en Mosquitto) ya no es el camino esperado en producción, así que se vuelve menos urgente de investigar más a fondo.
- Reemplaza: none (cierra `TASK-VAL-0020`; no cambia ningún veredicto de capacidad de DEC-0049/0050/0053 — el writer sigue sosteniendo ~36 commits/s, ahora el sistema es honesto al respecto en vez de aceptar de más silenciosamente)

### DEC-0055 — Corrección de métricas (filas vs. commits), descomposición del ciclo real, latencia end-to-end honesta, y hallazgo grave: sesión MQTT puede quedar atascada y perder el backlog en un reinicio

- Fecha: 2026-08-10
- Estado: `accepted` (hallazgo documentado; fix propuesto, no implementado — ver más abajo)
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: el usuario compartió un análisis propio que replantea "~36 commits/s" y sugiere medir 4 números (mensajes MQTT/s, operaciones SQLite/s, commits SQLite/s, latencia p50/p95/p99 end-to-end) y confirmar si ya hacemos group commit/batching. Al recalcular con ese marco aparecieron 3 hallazgos, uno de ellos serio.

**Corrección — "36 commits/s" era en realidad filas/s, no transacciones/s:** en la carga de DEC-0053 (65 batches, 6500 filas, 180s), el writer ya hace group commit correctamente (una transacción por batch de hasta 100 filas, `write_engine.rs`) — el número real de COMMITs (transacciones) fue 65/180s ≈ **0.36/s**, no 36/s. Esto invalida también el framing de "~2.77s por COMMIT" que se usó para plantear la hipótesis de esta fase — esa cifra mezclaba tiempo de ciclo completo con tiempo de commit real.

**Parte 1 — Descomposición del ciclo con datos reales (90s, throttle deshabilitado, 62 batches todos llenos a 100 filas, host Debian real):**

| Tramo | Promedio/batch | % del ciclo |
|---|---|---|
| Ciclo total (90s/62 batches) | 1.452s | 100% |
| `BATCH_FILL_DURATION` | 0.286s | 19.7% |
| `CACHE_SYNC_DURATION` | 0.150s | 10.3% |
| `WRITE_BATCH_DURATION` (COMMIT real) | 0.126s | 8.7% |
| **Sin explicar** | **0.890s** | **61.3%** |

**Hipótesis de esta fase REFUTADA con datos**: se sospechaba que el cambio de `cache_sync.rs` a despacho secuencial (DEC-0053) explicaba la mayor parte del ciclo — la medición directa lo descarta (solo 10.3%). El COMMIT real de SQLite tampoco es lento (126ms para 100 filas, razonable). El misterio real es **peor** que el 41% sin explicar de DEC-0050: 61.3% del ciclo no está en ninguna de las 3 métricas instrumentadas, y quitar tokio (DEC-0053) no lo cerró. No se investigó más a fondo en esta sesión (candidatos para una próxima: el propio `tracing::info!` con formato JSON hace I/O de escritura por línea, bajo journald eso puede bloquear; contención de CPU entre el hilo del writer y el hilo del consumidor MQTT en el mismo host).

No se ejecutó la Parte 2 del plan (restaurar paralelismo en `cache_sync.rs`) porque estaría optimizando un tramo que ya se sabe que es solo ~10% del ciclo — esfuerzo mal dirigido con esta evidencia en la mano.

**Parte 3 — Latencia end-to-end real con `ack_mode: committed`** (script nuevo `helpers/wrk/write_committed.lua`, rate-limiter recalibrado de DEC-0054 en su default de 40/s): contra el writer sano, **1,200 escrituras aceptadas y confirmadas, p50=14.55ms, p90=121.67ms, p99=185.20ms, max=343.97ms**. Es la primera vez en toda esta investigación que un número de latencia reportado es genuinamente end-to-end (espera la confirmación real vía `wait_for_commit`/`StatusQuery`, no solo el encolado a MQTT como con `ack_mode: accepted`).

**Hallazgo grave, encontrado por accidente durante la Parte 3 — una sesión MQTT puede quedar atascada bajo sobrecarga extrema, y el backlog se pierde silenciosamente si el writer se reinicia:**

Al ejecutar la Parte 3 justo después de la Parte 1 (que dejó a Mosquitto con ~100,093 mensajes en cola, su tope — `max_queued_messages`), el writer **dejó de procesar batches nuevos por completo durante 7+ minutos**, incluyendo 140 escrituras nuevas de una prueba de `wrk` y varias sondas directas — todas fallaron con `202 PENDING` tras 2000ms (el timeout de `WRITE_COMMITTED_TIMEOUT_MS`). Investigación:
- El proceso NO estaba colgado a nivel de hilos/CPU: `ps`/`/proc/<pid>/task/*/status` mostraron todos los hilos en `S` (durmiendo), ~1% CPU real (medido con 2 muestras de `/proc/<pid>/stat` separadas 3s), `voluntary_ctxt_switches` seguía incrementando (el hilo consumidor seguía despertando normalmente vía `epoll_wait`).
- El socket TCP del consumidor hacia Mosquitto (`/proc/net/tcp6`, IPv6 loopback) mostraba `rx_queue=0`/`tx_queue=0` en ambos extremos — **nada pendiente de leer o escribir**, en un momento en que Mosquitto reportaba 100,093 mensajes "stored". Es decir: el problema no es que el writer no lea datos que le llegan — es que Mosquitto no le está *enviando* nada.
- **Reiniciar el proceso writer no recuperó el backlog**: la nueva instancia (nuevo `client_id`, nueva sesión) arrancó limpia y procesó mensajes nuevos publicados en tiempo real casi instantáneamente (confirmado con un `mosquitto_pub` directo, batch procesado en <1s) — pero **nunca recibió ni un solo mensaje del backlog de ~100K**. `messages/dropped` de Mosquitto saltó de 5 (DEC-0053) a **121,591** durante esta sesión.

**Causa raíz probable** (circunstancial, fuerte, no confirmada con `strace`/debugger — no disponible en este entorno): `crates/ixmati-writer/src/main.rs` genera un `client_id` con un UUID aleatorio en cada arranque (`format!("writer-{}", uuid::Uuid::new_v4())`), y ni `consumer.rs` ni `event_publisher.rs` llaman `MqttOptions::set_clean_session(false)` — el default de `rumqttc` es `clean_session: true` (confirmado en el código fuente de la dependencia). Con sesión limpia, Mosquitto **descarta toda cola pendiente de esa sesión al reconectar/reiniciar** — por diseño, no es un bug de Mosquitto. La hipótesis de por qué la sesión ORIGINAL (sin reiniciar, mismo proceso, mismo `client_id` los 7+ minutos) dejó de recibir nada: `max_inflight_messages` de Mosquitto (default 20, no configurado explícitamente en `config/mosquitto/mosquitto.conf`) limita cuántos mensajes QoS1 pueden estar "en vuelo" sin PUBACK simultáneamente; si bajo la sobrecarga extrema de la Parte 1 alguno de esos hasta 20 mensajes en vuelo nunca fue confirmado (`client.ack()` en `consumer.rs` usa `flume::Sender::send()` internamente vía `rumqttc::Client`, una llamada **bloqueante** sobre un canal interno de capacidad 100 compartido con el mismo hilo que conduce el eventloop — bajo presión sostenida esto puede generar contención entre "encolar un ack" y "el eventloop drenando ese mismo canal", ambos en el mismo hilo), Mosquitto queda esperando esa confirmación indefinidamente y no envía nada más a esa sesión — coincide exactamente con todo lo observado (broker sin enviar nada, pero por lo demás sano; TCP idle en ambos lados; proceso nuestro sano y responsivo a tráfico nuevo).

**Esto es distinto del cuelgue silencioso del `WriteActor` (DEC-0051)**: acá el *proceso* nunca se congela — sigue vivo, sano, responde a tráfico nuevo casi al instante. Lo que se atasca es la *entrega* de una sesión MQTT específica del lado del broker, y la combinación con `clean_session=true` + `client_id` aleatorio hace que un reinicio (la respuesta obvia a "algo no fluye") **abandone el backlog en silencio en vez de recuperarlo** — sin ningún error visible para el operador más allá de las métricas `$SYS` de Mosquitto, que nadie está scrapeando activamente hoy.

**Fix propuesto (no implementado, pendiente de decisión)**: usar un `client_id` estable por store (ej. `format!("writer-{}", store_name)`, no un UUID aleatorio) + `mqtt_options.set_clean_session(false)` en `consumer.rs` (y evaluar lo mismo en `event_publisher.rs`). Con sesión persistente, un reinicio del writer reconecta a la MISMA sesión en Mosquitto y recibe automáticamente cualquier backlog pendiente en vez de perderlo. Efecto colateral a documentar: con `client_id` estable, si por error se llegan a correr dos instancias del writer para el mismo store simultáneamente, Mosquitto desconecta a la primera al conectar la segunda (mismo `client_id` no puede tener 2 sesiones activas) — en realidad esto refuerza la garantía de single-writer-por-store (DEC-0002) en vez de violarla, pero es un cambio de comportamiento observable que vale la pena anotar. No se investigó si el `max_inflight_messages` en sí necesita subir/configurarse explícitamente, ni se confirmó con 100% de certeza el mecanismo exacto del atasco (haría falta `strace`/`gdb` o un host sin restricción de `ptrace` anidado, no disponibles acá).

- Archivos: `helpers/wrk/write_committed.lua` (nuevo). Ningún cambio de código de producción en esta decisión — el fix queda propuesto, no aplicado.
- Consecuencias: (+) corrige un error de etiquetado propio (filas vs. commits) que venía arrastrándose desde DEC-0049. (+) primera medición honesta de latencia end-to-end real (p50=14.55ms, p90=121.67ms, p99=185.20ms) — útil para SLOs futuros. (+) refuta una hipótesis propia con datos en vez de asumir que estaba confirmada — evita una optimización mal dirigida en `cache_sync.rs`. (+) descubre un hallazgo de durabilidad real y no trivial: bajo sobrecarga extrema sostenida (el mismo escenario que DEC-0053/0054 usaron deliberadamente para medir el techo), un reinicio del writer puede perder en silencio un backlog completo de comandos ya aceptados por la API — sin ningún error visible salvo métricas de Mosquitto que hoy nadie scrapea. (-) el 61.3% del ciclo sin explicar sigue abierto, ahora con un candidato concreto a investigar (logging síncrono) pero sin confirmar. (-) el fix del `client_id`/`clean_session` no se implementó — queda como decisión pendiente del usuario, dado que toca comportamiento de reconexión que vale la pena confirmar antes de tocar.
- Reemplaza: none (corrige el etiquetado de DEC-0053/0054, no cambia sus veredictos de capacidad; abre un hallazgo nuevo no cubierto por ninguna decisión anterior)

### DEC-0056 — P0: sesión MQTT persistente (client_id estable + clean_session=false) — reduce la pérdida de backlog, no cierra el atasco de raíz

- Fecha: 2026-08-10
- Estado: `accepted` e implementado (parcial — ver hallazgo de validación)
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: primera prioridad del plan de priorización de DEC-0055 (`TASK-VAL-0024`) — pérdida de datos silenciosa es la categoría de severidad más alta, se ataca primero.

**Cambios de código**:
- `crates/ixmati-writer/src/main.rs` — `client_id` pasó de
  `format!("writer-{}", uuid::Uuid::new_v4())` (aleatorio en cada
  arranque) a `format!("ixmati-writer-{}", store_name)` (estable).
- `crates/ixmati-writer/src/consumer.rs` y
  `crates/ixmati-writer/src/event_publisher.rs` — `mqtt_options
  .set_clean_session(false)` en ambos (default de `rumqttc` era `true`,
  nunca sobreescrito).
- `crates/ixmati-writer/src/metrics.rs` — nuevo `Counter`
  `mqtt_ack_failures_total`, incrementado en los 2 sitios de
  `consumer.rs` donde un `client.ack()` fallido antes solo dejaba un
  `tracing::warn!` fácil de perder entre miles de líneas bajo carga.
- `config/mosquitto/mosquitto.conf` — `persistent_client_expiration 7d`,
  para que una sesión persistente de un `client_id` que deja de usarse
  (ej. tras renombrar un store) no quede retenida por Mosquitto para
  siempre — mismo efecto práctico que el `Session Expiry Interval` de
  MQTT v5, sin migrar de protocolo (evaluado y descartado por ahora:
  `rumqttc` sí trae un módulo `v5`, pero cambiar de protocolo es un riesgo
  mayor que no se justifica solo para esto).
- Verificado antes de implementar (no eran gaps): el camino de escritura
  ya usa `QoS::AtLeastOnce` de punta a punta, y `_idempotency` ya
  deduplica reentregas — sesión persistente + QoS1 + dedup es el modelo
  correcto sin necesitar QoS2/exactly-once.

**Verificación — mismo contenedor Debian real, mismo escenario de sobrecarga que DEC-0055** (throttle deshabilitado, `wrk` 90s, Mosquitto hasta su tope de cola):
- `cargo test --workspace --lib`: 8/8 crates en verde.
- Sobrecarga reproducida: 228,768 requests aceptadas, Mosquitto en 100,097
  mensajes almacenados, writer en su ritmo habitual (63 batches/6,300
  comprometidos antes de reiniciar).
- **Al reiniciar `ixmati-writer@default`: recuperación real, inmediata**
  — de 63 a 72 batches en 3 segundos, y siguió subiendo hasta 114 batches
  (Mosquitto bajó de 100,097 a 94,997 mensajes almacenados, ~5,100
  recuperados). **Antes de este fix (DEC-0055): 0 recuperados, backlog
  completo abandonado.** Esto por sí solo confirma que el mecanismo
  funciona — sesión persistente + `client_id` estable sí permite
  reentrega tras un reinicio.
- **Hallazgo honesto — el atasco volvió a ocurrir** a mitad del drenado:
  tras esos ~42 batches recuperados, el writer volvió a quedar en 0% CPU
  real (confirmado con 2 muestras de `/proc/<pid>/stat` separadas 3s,
  `ticks_delta=0`) y Mosquitto dejó de bajar (`94,997` estable en 2
  muestras separadas 3s) — el mismo patrón de atasco de DEC-0055, ahora
  ocurriendo *de nuevo* dentro de la misma sesión persistente.

- Consecuencias: (+) el fix cumple lo que promete — reduce drásticamente el blast radius de un reinicio bajo sobrecarga (de "pierde todo" a "recupera lo que puede hasta que se vuelve a atascar"), con evidencia real, no solo teórica. (+) confirma indirectamente que el mecanismo de sesión persistente de Mosquitto funciona como se esperaba con esta combinación de `client_id` estable + `clean_session=false`. (-) **no cierra el problema de raíz** — la causa de que la sesión se atasque en primer lugar (hipótesis de DEC-0055: `client.ack()` bloqueante compitiendo con el mismo hilo que debe drenar el canal interno de `rumqttc`, más `max_inflight_messages` de Mosquitto en su default de 20) sigue sin corregirse. Sin `P1`, un writer bajo sobrecarga sostenida puede seguir quedando indefinidamente atascado — ya no pierde lo ya atascado en el próximo reinicio, pero tampoco avanza solo. `P1` es indispensable, no opcional, y se implementa a continuación en esta misma sesión.
- Reemplaza: none (implementa P0 del plan de priorización de DEC-0055; el hallazgo de "atasco recurrente" motiva directamente P1)

### DEC-0057 — P1: `try_ack()` no bloqueante + `max_inflight_messages=200` — NO resolvió el atasco de sesión

- Fecha: 2026-08-10
- Estado: `accepted` (fix aplicado por ser de bajo riesgo y bien fundamentado, pero **no resuelve el problema que motivó esta decisión** — hipótesis de causa raíz de DEC-0055/0056 refutada)
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: P1 del plan de priorización de DEC-0055 — cerrar la causa de que la sesión MQTT del writer se atasque bajo sobrecarga (P0/DEC-0056 mitigó la pérdida de datos, pero el atasco reapareció a mitad de un drenado, confirmando que la causa de fondo seguía sin tocarse).

**Cambios de código**:
- `crates/ixmati-writer/src/consumer.rs` — `client.ack(&publish)`
  (bloqueante) → `client.try_ack(&publish)` (no bloqueante, `rumqttc`
  0.25.1 `client.rs:330`) en los 2 sitios donde se confirma un mensaje. Si
  el canal interno de `rumqttc` está lleno, el mensaje ya está a salvo en
  el canal acotado propio del writer — el broker lo reentrega más tarde,
  inocuo por QoS1 + dedup de idempotencia.
- `config/mosquitto/mosquitto.conf` — `max_inflight_messages 200` (default
  de Mosquitto era 20, nunca fijado explícitamente).

**Verificación — mismo contenedor Debian real, mismo escenario de sobrecarga que DEC-0055/0056** (throttle deshabilitado, `wrk` 90s, Mosquitto hasta su tope de cola), esta vez **sin reiniciar el writer**, observando 2 minutos completos con el mismo protocolo de diagnóstico:
- `cargo test --workspace --lib`: 8/8 crates en verde.
- Sobrecarga reproducida: 197,192 requests aceptadas, Mosquitto llegó a
  100,453 mensajes almacenados.
- **El atasco ocurrió de todos modos, sin necesidad de reiniciar**: tras
  51 batches procesados, el writer quedó en 0% CPU real (2 muestras de
  `/proc/<pid>/stat` separadas 3s: `ticks_delta=2` sobre 3s, ~0.67%) y
  Mosquitto se mantuvo exactamente en 100,453 mensajes almacenados
  durante **8 muestras consecutivas a lo largo de 2 minutos completos**
  (15s, 30s, 45s, 60s, 75s, 90s, 105s, 120s — todas idénticas). Todos los
  hilos del proceso en estado `S` (durmiendo), 0 errores/panics en el
  journal.

**Esto refuta la hipótesis de causa raíz de DEC-0055/0056**: si
`client.ack()` bloqueante y `max_inflight_messages=20` fueran la causa
principal, corregir ambos debería haber evitado o al menos retrasado
significativamente el atasco — no ocurrió así, se atascó en
aproximadamente el mismo punto (51 batches acá vs. 63-72 en DEC-0056, del
mismo orden de magnitud). La causa raíz real sigue sin identificarse. No
se investigó más a fondo en esta sesión — herramientas más profundas
(`strace`/`gdb` con acceso real, o logs de Mosquitto en nivel `debug`, que
no se activaron en este contenedor) serían el siguiente paso, no
disponibles en el tiempo de esta sesión.

- Consecuencias: (+) el cambio a `try_ack()` sigue siendo correcto en sí mismo (evita que el hilo que conduce el eventloop pueda bloquearse a sí mismo esperando espacio que solo el mismo hilo puede liberar) y se mantiene aunque no haya resuelto el atasco — es una mejora de robustez real, no un desperdicio. (+) `max_inflight_messages=200` tampoco se revierte — más margen no hace daño y sigue siendo una mitigación razonable para otros escenarios. (+) se evitó reportar un falso positivo — la tentación de declarar "P1 resuelto" tras ver el fix compilar y desplegarse fue descartada con la misma evidencia empírica exigida en toda esta investigación. (-) el atasco de sesión sigue sin causa raíz confirmada — con P0 en producción, un writer bajo sobrecarga sostenida sigue pudiendo quedar indefinidamente detenido hasta que un operador lo note y lo reinicie manualmente (P0 asegura que ese reinicio ya no pierde el backlog, pero no hay nada que dispare el reinicio automáticamente todavía — eso es tarea de `P3`, observabilidad). (-) esta sobrecarga es deliberadamente extrema (throttle desactivado a propósito, ~20-25x la capacidad real del writer) — no está confirmado si este escenario específico es realista en producción con el rate-limiter de DEC-0054 activo (40/s), donde la entrada nunca debería acumular un backlog de esta magnitud en primer lugar.
- Reemplaza: none (cierra la implementación de P1 del plan; dejar constancia de que la hipótesis de causa raíz no se confirmó, en vez de asumir que sí)

### DEC-0058 — P1.5: escalera de carga con `ack_mode: committed` — throughput sostenible real (dentro de un rango válido)

- Fecha: 2026-08-10
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: P1.5 del plan de priorización de DEC-0055 (propuesta del usuario) — reemplazar "~36-40 commits/s" por la pregunta correcta: ¿a qué tasa de entrada la cola empieza a crecer sin límite y el p99 se dispara? Script nuevo `helpers/wrk/staircase.sh`, 7 escalones (20/40/60/80/100/150/200 escrituras/s) fijando `MAX_WRITES_PER_WINDOW` por escalón, `ack_mode: committed` (latencia real, no solo aceptación), 30s por escalón, mismo contenedor Debian real.

**Resultados**:

| Objetivo | Aceptadas/s real | p50 | p90 | p99 | `outbox_size` (antes→después) |
|---|---|---|---|---|---|
| 20/s | 20.0/s exacto | 17.85ms | 24.30ms | 96.67ms | 0→0 |
| 40/s | 40.0/s exacto | 18.36ms | 37.07ms | 155.71ms | 0→0 |
| 60/s | 60.0/s exacto | 19.05ms | 108.82ms | 239.39ms | 0→0 |
| 80/s | 80.0/s exacto | 19.56ms | 132.09ms | 226.86ms | 0→0 |
| 100/s | 100.0/s exacto | 20.96ms | 216.62ms | 320.11ms | 0→48 |
| 150/s | **136.0/s** (no llegó a 150) | 275.00ms | 372.27ms | 542.99ms | 0→48 |
| 200/s | **115.2/s** (no llegó a 200, ni superó al de 150) | 408.25ms | 509.69ms | 609.62ms | 0→48 |

`consumer_queue_depth` se mantuvo en 0 en los 7 escalones — el canal acotado del consumidor MQTT nunca llegó a acumular nada en este rango.

**Limitación de metodología encontrada (a diferencia de los escalones 20-100, los de 150/200 NO son mediciones válidas de "qué pasa a esa tasa de entrada")**: `wrk` usa concurrencia fija (`-c50`) durante toda la corrida — no tiene control nativo de tasa. En los escalones 20-100, las aceptadas/s coinciden exactamente con el objetivo (confirmando que el rate-limiter, no `wrk`, era el limitante real, tal como se buscaba). Pero en el escalón de 150, la latencia real ya subió tanto (`p50`≈275ms) que **50 conexiones concurrentes no alcanzan a generar 150 requests/s** (el techo teórico con esa latencia y esa concurrencia es ≈180/s, y el observado fue menor aún, 136/s) — a partir de ahí, la métrica que se está midiendo es el límite de la propia herramienta de carga, no el límite de Ixmati. El escalón de 200 confirma esto: 0 rechazos por `429` (`errors_status=0`) porque el rate-limiter de 200/s nunca llegó a activarse — `wrk` ni siquiera pudo generar tráfico suficiente para acercarse.

**Conclusión honesta, solo sobre el rango válido (20-100/s)**: `p50` se mantiene notablemente bajo y estable en todo el rango (17.85ms→20.96ms, sin degradación marcada) — pero la cola (`p90`/`p99`) empieza a degradarse de forma visible a partir de **60/s** (`p90` salta de 37ms a 109ms) y ya es pronunciada en **100/s** (`p90`=216ms, `p99`=320ms), sin que `outbox_size`/`consumer_queue_depth` muestren un crecimiento descontrolado (`outbox_size` aparece en 48 a partir de 100/s pero **no sigue creciendo** en los escalones siguientes — un residuo estable, no una fuga). Esto sugiere que la degradación de cola en 60-100/s no es por acumulación de backlog en las colas medidas, sino por otra causa no identificada (candidato: contención de lecturas concurrentes de `StatusQuery::query` contra SQLite desde múltiples peticiones `ack_mode: committed` en paralelo, cada una haciendo polling cada 30ms — no confirmado, requeriría instrumentación nueva). El rate-limiter actual de producción (40/s, DEC-0054) queda claramente dentro de la zona saludable de esta escalera.

**No se determinó el punto real de quiebre** (donde la cola crece sin límite) — la escalera necesitaría repetirse con mayor concurrencia de `wrk` (`-c150`+ o una herramienta con control de tasa real, ej. `wrk2` o `vegeta`) para poder probar honestamente los escalones de 150-200/s y más allá.

- Archivos: `helpers/wrk/staircase.sh` (nuevo, generalizado para reutilizarse — acepta host/puertos/contenedor por parámetro/env var en vez de hardcodearlos a esta sesión).
- Consecuencias: (+) reemplaza "~36-40 commits/s" (una medida de saturación deliberada, sin sentido operativo directo) por datos de latencia real bajo tasas de entrada controladas — mucho más útil para fijar SLOs. (+) confirma que el rate-limiter actual (40/s) opera en una zona con buen `p50` pero ya con algo de cola visible en `p90`/`p99` — no hay margen enorme antes de que la cola empiece a notarse. (+) descubre honestamente una limitación de la propia metodología de prueba (concurrencia fija de `wrk`) en vez de reportar 150/200 como si fueran mediciones válidas del sistema. (-) no se encontró el punto de quiebre real (crecimiento de cola sin límite) — queda abierto, requiere una corrida con mayor concurrencia o una herramienta con control de tasa. (-) la causa de la degradación de `p90`/`p99` entre 60-100/s sin crecimiento de las colas medidas no se investigó a fondo — candidato anotado (contención de `StatusQuery::query` bajo polling concurrente) pero no confirmado.
- Reemplaza: none (cierra P1.5 del plan de priorización de DEC-0055 dentro del rango válido probado; dejar constancia explícita de que 150/200 no son datos confiables en vez de reportarlos sin la salvedad)

### DEC-0059 — Contrato de durabilidad: ACK después de SQLite y PUBACK después del broker

- Fecha: 2026-08-10
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Contexto: la revisión de la escalera y del atasco MQTT confirmó que había
  dos falsos límites de éxito: el consumidor confirmaba al broker al poner el
  comando en una cola volátil de RAM, y el publicador marcaba el outbox como
  publicado al encolarlo en rumqttc, antes del PUBACK. Además, la API podía
  responder `200 ACCEPTED` sin confirmación de SQLite.

**Decisión**:

- `ixmati-writer` conserva el token de ACK MQTT junto con cada comando y sólo
  lo envía después de que el batch termina correctamente en SQLite. Si la
  transacción falla o el proceso muere antes de ese punto, el mensaje no se
  confirma y QoS1 lo reentrega; la idempotencia hace segura la repetición.
- `EventPublisher` sólo ejecuta `mark_published_batch` para ids cuyo PUBACK
  fue observado en el eventloop de rumqttc. Un reinicio entre PUBACK y el
  `UPDATE` puede producir duplicación de eventos, pero no pérdida; el contrato
  de eventos es at-least-once.
- La API trata `ack_mode=accepted` como alias compatible de `committed` y sólo
  devuelve `200 OK` cuando `_idempotency` confirma el commit. Si no puede
  confirmarlo dentro del timeout devuelve `202 PENDING`; sin `SQLITE_PATH`
  rechaza la escritura porque no puede prometer el contrato.
- La sincronización de cache queda deliberadamente después del ACK MQTT y es
  best-effort: SQLite es la fuente durable y el cache es reconstruible. Las
  fallas de cache se cuentan y registran, pero no convierten una escritura
  durable en una pérdida.

**Consecuencias**: (+) el significado de éxito es verificable y resistente a
  caída del writer/API en las ventanas críticas; (+) las redeliveries quedan
  cubiertas por la idempotencia existente; (+) las métricas de cola ya tienen
  call sites reales y la escalera no transforma una métrica ausente en cero;
  (-) puede aumentar la latencia observada y dejar `202` bajo presión; (-) el
  outbox es at-least-once y puede duplicar eventos después de un crash entre
  PUBACK y su marca SQLite; (-) el punto real de saturación aún requiere una
  corrida con un generador rate-controlled como wrk2/vegeta.
- Reemplaza: la semántica optimista documentada implícitamente por
  `ack_mode=accepted` y la marca de outbox basada sólo en enqueue local.

La revisión posterior detectó además que un único `SQLITE_PATH` no alcanza
para confirmar escrituras multi-store. El API ahora acepta `SQLITE_PATHS` con
el mapa explícito `store=path` y el compose multi-store monta cada base; sin
ese mapa una escritura a un store sin ruta se rechaza en vez de responder un
éxito no verificable.

### DEC-0060 — Validación final rate-controlled y crash/restart en Debian amd64

- Fecha: 2026-08-11
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0030`, `TASK-VAL-0031`
- SHA validado: `6eaa80a3e083b0974886e9d6ab1f0172c8303e19`

**Contexto**: se publicó el SHA exacto en `origin/main`, se construyó el
tarball `linux-amd64` desde ese SHA y se instaló idempotentemente en el host
Debian remoto. El host no tenía `wrk2`; por ello la escalera usó el generador
estándar `helpers/python/rate_load.py`, que mantiene una tasa global fija y
reporta saturación del propio generador. El throttle nominal de producción
fue `40/s`.

**Resultados de carga**:

| Objetivo | Throughput observado | p50 | p90 | p99 | HTTP 200/202/429 |
|---:|---:|---:|---:|---:|---:|
| 20/s | 20.0/s | 145ms | 186ms | 283ms | 572/0/28 |
| 40/s | 40.0/s | 605ms | 1602ms | 2052ms | 1110/48/42 |
| 60/s | 60.0/s | 767ms | 1536ms | 1858ms | 1740/0/60 |
| 80/s | 80.0/s | 892ms | 1428ms | 1601ms | 2294/0/106 |
| 100/s | 100.0/s | 919ms | 1394ms | 1550ms | 2891/0/109 |
| 150/s | 119.1/s | 1960ms | 2095ms | 2216ms | 1940/1261/371 |
| 200/s | 100.0/s | 2066ms | 2101ms | 2161ms | 100/2901/0 |

Los escalones 20–100 mantuvieron la tasa objetivo y son mediciones válidas
del comportamiento bajo esas tasas. Los escalones 150 y 200 son
inconclusos para capacidad del servidor: `client_saturated_ticks` fue 1032 y
2006 respectivamente, por lo que el generador no pudo producir la tasa
solicitada. `consumer_queue_depth` no mostró acumulación durante la corrida;
las series ausentes de `outbox_size` se conservaron como `NA`, no como cero.
El resultado de 40/s contradice la conclusión de zona saludable de DEC-0058
(`p99`≈2.05s y 48 respuestas `202`) y obliga a investigar la regresión antes
de usar 40/s como SLO de producción.

**Resultados de crash/restart**: durante la ingestión se publicaron 30 claves
conocidas, se terminó el writer con `SIGKILL` y se reinició mediante systemd.
Las 30/30 escrituras quedaron `APPLIED` en la API, las 30/30 claves estuvieron
presentes en `_idempotency` y se observaron 30/30 eventos en el suscriptor
MQTT. No se observó pérdida; los duplicados siguen siendo permitidos por el
contrato at-least-once. Una corrida anterior de 100 mensajes se descartó
porque el contenedor mínimo no tenía el binario `sqlite3` y, por tanto, no
verificaba SQLite. La ventana exacta entre PUBACK y `published_at` no se
forzó determinísticamente y permanece pendiente como prueba específica.

**Consecuencias**: (+) la validación ahora separa throughput sostenible de
resultados limitados por el cliente y deja evidencia reproducible por SHA;
(+) el reinicio durante tráfico no mostró pérdida de escrituras ni eventos;
(-) la capacidad sostenible real y la regresión observada a 40/s siguen
requiriendo investigación; (-) no se debe extrapolar 150/200 sin `wrk2` o un
generador equivalente con concurrencia suficiente; (-) la ventana PUBACK→
`published_at` requiere inyección de fallo controlada.
- Reemplaza: la interpretación optimista de DEC-0058 sobre 40/s y sus
  escalones altos basados en `wrk` con concurrencia fija.

### DEC-0061 — Límite temporal del batcher y concurrencia productiva validada

- Fecha: 2026-08-11
- Estado: `accepted`
- Relacionado con specs: `SPEC-VAL-0001`
- Relacionado con tareas: `TASK-VAL-0025`, `TASK-VAL-0032`
- SHA validado: `9d96b0b34c2cfa83b717d6cb79e3e065c35f674c`

**Contexto**: DEC-0060 mostró una regresión severa a 40/s. La instrumentación
del writer explicó el síntoma: `batch_fill_duration` promediaba ~807ms,
mientras SQLite promediaba ~20ms y cache ~41ms. El `Batcher` sólo evaluaba
`should_flush()` después de un `RecvTimeoutError::Timeout`; si llegaba un
comando antes de cada timeout, el flujo continuo impedía el flush temporal y
el batch esperaba llenarse.

**Decisión**:

- `Batcher::push()` vacía el batch cuando alcanza `batch_size` o cuando
  `batch_interval` ya venció, incluso si siguen llegando comandos. El timeout
  del canal continúa cubriendo el caso de tráfico intermitente.
- La espera de confirmación SQLite del API se ejecuta con
  `tokio::task::spawn_blocking` y una conexión reutilizada durante cada
  espera. Esto evita I/O síncrono en los workers async y elimina un
  `Connection::open` por cada polling de 30ms. El contrato no cambia:
  `_idempotency` sigue siendo la fuente de verdad y `202` sigue significando
  `PENDING`.

**Prueba de regresión**: `flushes_on_interval_even_when_commands_keep_arriving`
  reproduce el caso que antes no vaciaba el batch. `cargo test -p ixmati-writer
  --lib`, `cargo test -p ixmati-api --lib` y `make ci-main` pasan.

**Validación Debian amd64**: se construyó el tarball desde el árbol del SHA
  indicado, se instaló en un contenedor Debian limpio y se ejecutó el
  generador rate-controlled con concurrencia 200/300 durante 30s por
  escalón. Los escalones 60–120 usaron un throttle temporal de 1000/s para
  medir capacidad; la corrida de 40/s usó el throttle de producción.

| Objetivo | Throughput | p50 | p90 | p99 | HTTP 200/202/429 | Cola MQTT |
|---:|---:|---:|---:|---:|---:|---:|
| 40/s | 40.0/s | 79ms | 113ms | 147ms | 1165/0/35 | 0 |
| 60/s | 60.0/s | 110ms | 147ms | 205ms | 1800/0/0 | 0 |
| 80/s | 80.0/s | 161ms | 235ms | 281ms | 2400/0/0 | 0 |
| 100/s | 100.0/s | 218ms | 300ms | 375ms | 3000/0/0 | 0 |
| 120/s | 120.0/s | 128ms | 188ms | 252ms | 3600/0/0 | 0 |
| 150/s | 147.4/s | 2032ms | 2052ms | 2125ms | 842/3580/0 | 100 |

Los escalones 40–120 son capacidad productiva observada: alcanzaron la
tasa solicitada, no tuvieron saturación del generador, no devolvieron `202`
y la profundidad del consumidor terminó en cero. El escalón de 150/s es el
primer punto de saturación; tuvo `client_saturated_ticks=2486`, por lo que su
throughput exacto no es una medición pura del servidor, pero sí demuestra que
el sistema deja de sostener la tasa y acumula cola. La prueba de 120/s se
repitió en un contenedor limpio; una corrida previa contaminada por el
backlog de 150/s fue descartada.

**Consecuencias**: (+) el rate limiter de producción de 40/s queda con
latencia observada muy inferior a DEC-0060; (+) se demuestra concurrencia
productiva hasta 120/s en el hardware Debian probado; (+) el límite de
latencia del batch queda protegido por diseño y por test; (-) 150/s no es
sostenible y requiere backpressure/escala horizontal o más capacidad por
store; (-) el outbox PUBACK sigue siendo at-least-once y su ventana de crash
determinista permanece pendiente.
- Reemplaza: la explicación abierta de DEC-0060 sobre la regresión a 40/s y
  la hipótesis no confirmada de que la degradación provenía principalmente
  del polling SQLite.

### DEC-0062 — Validación e2e de lecturas, cache y proyecciones

- Fecha: 2026-08-11
- Estado: `accepted`
- Relacionado con tareas: `TASK-VAL-0036`
- Evidencia: `spec-native/evidence/READ-CACHE-20260811.md`

**Contexto**: la validación de escritura no demostraba que el camino de
lectura multi-store funcionara bajo carga ni después de reiniciar el
cache-server. El compose montaba los volúmenes SQLite en rutas que el API no
podía abrir como archivos, y `CacheClient` conservaba un socket roto después
de un reinicio.

**Decisión**:

1. El compose multi-store monta cada volumen en `/data/<store>` y configura
   `SQLITE_PATHS` con `/data/<store>/<store>.db`.
2. `CacheClient` reconecta automáticamente una vez después de detectar una
   desconexión en GET, SET, DEL, `DEL_PREFIX` o FLUSH.
3. El health check resuelve nombres DNS de brokers antes de probar TCP.
4. La validación de lectura usa respuestas y payloads reales, no sólo códigos
   HTTP: cache-aside y Pattern M sostuvieron 100–500/s sin errores; a 1000/s
   no hubo errores, pero el generador mostró saturación y degradación de
   latencia en cache-aside.
5. Pattern R se considera válido para construir el snapshot inicial, pero no
   para relaciones mutables: un cambio en una entidad referenciada no refresca
   automáticamente todos los read models existentes. Se mantiene como
   limitación documentada y queda pendiente fan-out, índice inverso o lookup
   en lectura.

**Consecuencias**: (+) cache-aside tiene fallback y repoblación después de un
reinicio del cache-server sin reiniciar el API; (+) las vistas M y R iniciales
quedan verificadas en Debian amd64; (+) el health check refleja correctamente
un broker con hostname; (-) Pattern R no es actualmente seguro como vista viva
cuando cambian sus stores referenciados; (-) 1000/s no es un SLO sostenible
porque el cliente de prueba se saturó, aunque no hubo respuestas erróneas.

### DEC-0063 — Comparativa reproducible de capacidad entre motores

- Fecha: 2026-08-11
- Estado: `accepted`
- Relacionado con tareas: `TASK-VAL-0037`
- Evidencia: `spec-native/evidence/DB-COMPARISON-20260811.md`

**Contexto**: los números anteriores medían el pipeline completo de Ixmati,
pero no separaban el costo de la base SQLite directa ni ofrecían una línea de
comparación con PostgreSQL bajo el mismo hardware, dataset y tasa de llegada.

**Decisión**:

1. La suite usa un esquema equivalente con `payload_usuarios`,
   `payload_pedidos`, `_idempotency` y `_outbox` en SQLite y PostgreSQL.
2. SQLite directo usa `WAL`, `synchronous=NORMAL` y `busy_timeout=5000`;
   PostgreSQL usa `synchronous_commit=on`.
3. Los escenarios incluyen lecturas puntuales, lecturas con relación,
   inserciones, actualizaciones, idempotencia y carga mixta.
4. Cada escenario usa warmup, tasa controlada, tres repeticiones, p50/p95/p99,
   errores y detección de saturación del cliente.
5. Las referencias oficiales de PostgreSQL se muestran en una sección
   separada. El ejemplo de `pgbench` con 896.967 TPS y 11.013 ms es
   ilustrativo y no se trata como capacidad de este host; la afirmación de
   PostgreSQL 17 de hasta 2x en throughput de escritura bajo alta
   concurrencia tampoco se mezcla con mediciones locales.

**Consecuencias**: (+) la tabla permite distinguir base de datos, pipeline,
cache, proyecciones y durabilidad; (+) el resultado queda reproducible por
   SHA con manifiesto, configuración y JSON crudos; (-) PostgreSQL directo no
   es equivalente al pipeline completo de Ixmati; (-) `cold-first-pass` no
   implica vaciar la page cache del kernel; (-) Pattern R mutable permanece
   fuera de los escenarios aceptados para producción.

**Resultado de la ejecución 2026-08-11**: con 1,000 usuarios y 10,000
pedidos, SQLite directo sostuvo 1,000 lecturas/s y 200 escrituras/s en este
workload; PostgreSQL 18 sostuvo 500 lecturas/s puntuales y 200 escrituras/s
antes de saturar el cliente en lecturas de mayor concurrencia; Ixmati sostuvo
1,000 lecturas/s cacheadas/proyectadas sin errores y aproximadamente 40
escrituras durables/s. A partir de 40–60 escrituras/s el API mostró `429` y
`PENDING`, sin aumento equivalente del commit rate. Estos números son
capacidad observada en este host y configuración, no garantías universales.
La ejecución mostró además que `consumer_queue_depth` no está expuesta en el
endpoint `/metrics`; su ausencia se dejó como gap de observabilidad, no como
valor cero.

### DEC-0064 — Posicionamiento del producto según la comparativa de capacidad

- Fecha: 2026-08-11
- Estado: `accepted`
- Relacionado con tareas: `TASK-VAL-0037`, `TASK-VAL-0025`, `TASK-VAL-0034`
- Evidencia: `spec-native/evidence/DB-COMPARISON-20260811.md`

**Contexto**: la comparativa separó el rendimiento del motor SQLite del costo
del pipeline completo de Ixmati. SQLite y PostgreSQL directos tienen menos
capas que Ixmati y, por tanto, sus números no representan el mismo producto.

**Decisión**: Ixmati se posiciona como una capa durable de escritura y
aceleración de lecturas sobre SQLite, no como un motor de SQL de throughput
bruto superior a SQLite o PostgreSQL. La capacidad observada en la
comparativa completa queda documentada así:

1. El camino de lectura cacheado y proyectado sostuvo 1,000 operaciones/s
   con p99 aproximado de 1.6 ms en el workload de Debian, sin saturación del
   generador.
2. El camino de escritura durable confirmó aproximadamente 40 escrituras/s
   con el throttle productivo; ofrecer más carga produjo `429`/pendientes sin
   aumentar el commit rate.
3. La latencia p99 cercana a 2 s en las escrituras `ack_mode=committed` es un
   cuello de botella actual del camino durable. No se presenta como un SLO
   deseado ni se oculta detrás del throughput.
4. El producto es adecuado para cargas single-host o edge con escritura
   durable moderada, alta fan-out de lectura, idempotencia y outbox. No se
   presenta todavía como solución para workloads write-heavy ni como
   reemplazo general de PostgreSQL.
5. Los `429` bajo sobrecarga se consideran backpressure correcto y observable,
   siempre que el límite, la latencia y la recuperación estén documentados y
   monitorizados; aceptar trabajo que luego no se persiste sería peor.

**Consecuencias**: (+) la propuesta de valor queda alineada con evidencia
real: coordinación durable de productores, outbox, idempotencia, cache y
proyecciones; (+) los baselines directos sirven para cuantificar el costo de
esas garantías; (-) antes de reclamar alto throughput de escritura hay que
reducir la cola/latencia del writer y completar la observabilidad; (-) la
prueba determinista de crash entre PUBACK y `published_at` sigue siendo
necesaria para cerrar la afirmación de entrega sin pérdida.
- Reemplaza: cualquier lectura que interprete los resultados directos como
  prueba de que Ixmati supera a SQLite/PostgreSQL en capacidad bruta, o que
  presente 100–200 escrituras/s del API como capacidad durable productiva.

### DEC-0065 — Índice inverso para mantener Pattern R mutable

- Fecha: 2026-08-11
- Estado: `accepted`
- Relacionado con tareas: `TASK-VAL-0036`

**Contexto**: Pattern R construía correctamente la vista inicial, pero sólo
reconstruía cuando cambiaba el evento del store primario. Una actualización de
una entidad referenciada dejaba el read model con el valor anterior.

**Decisión**:

1. El projector mantiene un índice interno en cache bajo el namespace
   `ridx:<projection>:<store>:<entity>:<key>`. El valor es una lista JSON de
   claves de proyección afectadas; no forma parte de la API pública.
2. Al procesar un evento del store primario, Pattern R registra cada clave de
   referencia y la clave de la proyección. El fan-out queda limitado a 100
   dependientes, igual que la regla operativa vigente para decidir entre R y M.
3. Al procesar un evento de un store referenciado, el projector consulta el
   índice, reconstruye todas las vistas afectadas y usa el payload del evento
   como valor autoritativo para evitar una carrera con `cache_sync`.
4. Los eventos de eliminación eliminan las vistas afectadas. Los eventos fuera
   de orden se descartan dentro de la vida del projector cuando tienen una
   versión menor que la ya procesada para la misma entidad.
5. `ixmati-reconciler` borra y reconstruye el índice junto con cada Pattern R.
   La pérdida total de cache, un reinicio del projector o un cambio de clave de
   referencia se recuperan mediante reproyección completa.

**Consecuencias**: (+) una actualización referenciada deja de requerir una
lectura en tiempo real ni una modificación de la API; (+) el índice es
reconstruible y el límite de fan-out es explícito; (+) la consistencia sigue
siendo eventual, pero después de procesar el evento la vista contiene el nuevo
valor; (-) una relación con más de 100 dependientes no se propaga
automáticamente y debe usar Pattern M, partición del modelo o una política de
negocio distinta; (-) la deduplicación de versiones es en memoria hasta que el
reconciler vuelva a reconstruir el estado persistente.
- Reemplaza: la limitación documentada en DEC-0062 de Pattern R mutable sólo
  como snapshot inicial.

### DEC-0066 — Evidencia inicial del supuesto atasco MQTT y del coste SQLite

- Fecha: 2026-08-11
- Estado: `accepted`
- Relacionado con tareas: `TASK-VAL-0035`, `TASK-VAL-0025`
- Evidencia: `spec-native/evidence/MQTT-STALL-DIAGNOSTIC-20260811.md`

**Contexto**: se repitió la sobrecarga con 180,000 comandos MQTT directos y
se capturó `strace -f` sobre el writer Debian. El broker llegó a cerca de
100,000 mensajes almacenados, pero el writer continuó procesando y la cola
descendió. No se reprodujo el estado histórico de proceso vivo sin progreso.

**Decisión**:

1. No se declara resuelto el atasco histórico ni se cambia el transporte MQTT
   sin una reproducción específica. `TASK-VAL-0035` permanece pendiente.
2. En la corrida inicial, `batch_ack_duration_seconds` promedió unos 45 µs por
   batch, frente a unos 413 ms de procesamiento SQLite y 96 ms de cache. El
   cuello observado es el camino durable SQLite bajo backlog, no el envío del
   `PUBACK`.
3. Se corrige la trazabilidad de DEC-0057: su prueba de `try_ack()` correspondía
   al diseño previo. Desde `5cccb91`, el camino durable conserva el ACK junto
   al comando y llama a `Client::ack()` después de SQLite; por tanto la prueba
   P1 histórica no cubre el camino actual.
4. El probe incorpora captura opcional de syscalls (`STRACE=1`) y conserva la
   evidencia para la siguiente corrida con un generador externo no saturado.

**Consecuencias**: (+) evita atribuir a MQTT un tiempo que las métricas no
   muestran; (+) identifica una regresión/lag documental importante entre
   DEC-0057 y el diseño durable vigente; (+) la recuperación por watchdog
   sigue siendo una defensa válida para pérdida de progreso; (-) la causa del
   bloqueo histórico del broker permanece sin confirmar; (-) la latencia de
   SQLite bajo backlog sigue siendo un riesgo de capacidad y requiere una
   investigación separada antes de subir el límite productivo.
- Reemplaza: no reemplaza DEC-0057; delimita su alcance experimental. La
  resolución posterior del cuello SQLite está registrada en DEC-0067.

### DEC-0067 — Índice faltante en `_idempotency` era la causa del atasco aparente

- Fecha: 2026-08-11
- Estado: `accepted`
- Relacionado con tareas: `TASK-VAL-0035`, `TASK-VAL-0025`
- Evidencia: `spec-native/evidence/MQTT-STALL-DIAGNOSTIC-20260811.md`

**Contexto**: `IdempotencyTracker::current_version()` consultaba
`MAX(version)` filtrando por `(store, entity, key)`, pero `_idempotency` sólo
tenía la clave primaria `(store, idempotency_key)`. Cada comando podía recorrer
las filas del store completo. Bajo backlog MQTT esto hacía crecer el tiempo de
SQLite hasta parecer un bloqueo del consumidor y del broker.

**Decisión**:

1. `ensure_schema()` crea de forma idempotente el índice covering
   `idx_idempotency_entity_key_version(store, entity, key, version)`. La
   operación sirve tanto para bases nuevas como para bases existentes y no
   cambia el contrato de idempotencia.
2. Se añadió una regresión que verifica el plan `EXPLAIN QUERY PLAN` y otra que
   verifica la migración de una tabla `_idempotency` ya creada.
3. En Debian amd64, con la misma base de 50,000 filas, el plan pasó de usar
   únicamente `sqlite_autoindex__idempotency_1 (store=?)` a usar el índice
   covering. Con 20,000 comandos adicionales, SQLite procesó 398 batches en
   3.49 s (8.8 ms/batch promedio), frente a la corrida sin índice que llegó a
   aproximadamente 200 ms/batch al crecer la base.

**Validación posterior**: la corrida rate-controlled de `cc7b912` confirmó el
perfil productivo a 40/s con p99 de 143.9 ms y cero saturación del generador.
La escalera 20–200/s mantuvo `consumer_queue_depth=0`; el rango 40–100/s quedó
por debajo de 160 ms p99 y 150/s presentó 321 ms p99. Los datos completos están
en `spec-native/evidence/LOAD-POST-INDEX-20260811.md`.

**Consecuencias**: (+) el cuello observado queda atribuido a un error de
diseño de acceso SQLite, no a un cambio de transporte MQTT; (+) la migración
es automática y reproducible; (+) el consumidor puede drenar el backlog sin
que el coste de deduplicación crezca linealmente con el store; (-) el índice
añade espacio y coste de escritura; (-) la capacidad final debe repetirse con
el artefacto publicado y una carga controlada. El watchdog sigue siendo una
defensa operativa, no la corrección primaria.
- Reemplaza: la conclusión provisional de DEC-0066 sobre causa no confirmada.

### DEC-0068 — Migración offline y determinista del ciclo de vida de stores

- Fecha: 2026-08-12
- Estado: `accepted`
- Relacionado con: `TASK-STORE-0001`..`TASK-STORE-0006`
- Implementación: `crates/ixmati-store-migrate`, `spec-native/RUNBOOK-STORE-MIGRATION.md`

**Decisión**: rename, merge y split son operaciones offline con outbox drenado,
integridad SQLite, backup verificable y publicación atómica de destinos
temporales. Merge usa LWW por versión, timestamp y nombre de origen
lexicográficamente menor; las colisiones de idempotencia con digest divergente
aborta la operación. Las eliminaciones se conservan como `_tombstones` y los
históricos ambiguos no se resuelven optimistamente. Split usa
`sha256-key-v1` sobre la clave y la lista ordenada de destinos. El cutover es
estricto, sin alias ni doble publicación; reconciler reconstruye cache y
proyecciones después de la migración.

**Consecuencias**: se requiere ventana de mantenimiento y no existe
transacción distribuida ni migración online, pero la operación es reproducible,
auditable y deja el origen intacto ante un fallo.

### DEC-0069 — Interfaces Protobuf sobre el núcleo durable

- Fecha: 2026-08-12
- Estado: `accepted`
- Relacionado con: `SPEC-PROTOBUF-0001`, `TASK-PROTO-0001`..`0006`, `TASK-WRITE-0008`
- Implementación: `crates/ixmati-api/src/grpc.rs`, `proto/ixmati/v1/`, `crates/ixmati-api/src/rest.rs`

**Decisión**: Ixmati expone gRPC en un listener separado (`GRPC_PORT=30100`)
y REST binario mediante `application/protobuf`, manteniendo REST/JSON. El
payload canónico es `google.protobuf.Struct` en el campo wire 10; el campo
wire 9 se conserva como `payload_bytes` deprecated para compatibilidad con
clientes anteriores. gRPC autentica con metadata
`x-api-key` reutilizando `IXMATI_API_KEYS`. `accepted` sigue siendo alias
durable de `committed`; `COMMITTED`/HTTP 200 sólo representa confirmación en
`_idempotency` y `PENDING`/HTTP 202 requiere consulta de estado.

`EventService.SubscribeEvents` usa el `id` de `_outbox` como cursor durable,
reproduce sólo la retención disponible y continúa con eventos nuevos. El
servidor registra el stream antes de leer, aplica filtros, deduplica dentro de
una ventana acotada y usa un buffer por cliente. La entrega es at-least-once:
un cursor fuera de retención devuelve `OUT_OF_RANGE` y un consumidor que no
puede mantenerse al día se termina con `RESOURCE_EXHAUSTED`. No se habilitan
reflection ni el health protocol estándar; `HealthService.Check` conserva el
health propio de Ixmati.

**Consecuencias**: (+) clientes REST, gRPC y Protobuf comparten la misma
durabilidad, cache, SQLite, MQTT y autenticación; (+) el contrato binario no
duplica el modelo de negocio; (-) el stream depende de la retención del
outbox y no ofrece exactamente-una-entrega; (-) el benchmark de la nueva
interfaz debe demostrar impacto sobre la confirmación durable antes de
atribuirle una mejora de capacidad. El listener puede deshabilitarse con
`GRPC_PORT=0` durante una migración legacy.

**Resultado de validación**: en Debian amd64, con `ack_mode=committed`, 200
clientes y tasa controlada, JSON, REST/Protobuf y gRPC confirmaron 39.1/s en el
baseline de 40/s, con el mismo número de rechazos del throttle y sin saturación
del generador. A 100/150/s los tres caminos quedaron limitados a 40/s por
backpressure. La evidencia completa está en
`spec-native/evidence/PROTOBUF-BENCH-20260812.md`; no se atribuye mejora de
capacidad a los protocolos.

### DEC-0070 — Perfil operativo con margen y readiness explícito

- Fecha: 2026-08-12
- Estado: `replaced`
- Relacionado con: `TASK-PROD-0001`, `TASK-VAL-0020`

El límite predeterminado de admisión se fija en 30 escrituras por segundo y
store, con ventana de un segundo. El techo observado de aproximadamente 40/s
se conserva como escalón de diagnóstico, no como SLO. La respuesta HTTP 429
incluye `Retry-After`; gRPC devuelve el mismo dato como metadata. Los clientes
deben reintentar con la misma clave de idempotencia.

Se añade `GET /ready`, que devuelve 200 sólo cuando MQTT y todos los SQLite
configurados están disponibles; `/health` conserva HTTP 200 como endpoint de
diagnóstico. Esto separa liveness/diagnóstico de readiness para systemd,
balanceadores y despliegues supervisados.

La decisión reduce el riesgo de operar sobre el borde del writer y convierte
el backpressure en un contrato recuperable. No aumenta la capacidad bruta ni
sustituye la prueba prolongada de una hora; esa validación queda en
`TASK-PROD-0001`.

### DEC-0072 — Perfil productivo reducido a 25/s tras soak fallido a 30/s

- Fecha: 2026-08-12
- Estado: `replaced`
- Reemplaza: `DEC-0070`
- Relacionado con: `TASK-PROD-0001`, `TASK-VAL-0020`, `DEC-0071`

La corrida de validación con 200 clientes y tasa controlada de 30/s confirmó
que el sistema conserva durabilidad eventual, pero no cumple el contrato de
confirmación operativa: en aproximadamente 20 minutos hubo 35,510 respuestas
`COMMITTED` y 1,625 `PENDING` por timeout de 2 segundos (4.4%). Las claves
pendientes terminaron presentes en `_idempotency`, el outbox quedó drenado y
SQLite pasó `integrity_check`; por tanto el hallazgo es de latencia y margen,
no de pérdida de datos.

El valor predeterminado se reduce a `MAX_WRITES_PER_WINDOW=25` por store. El
token bucket y `Retry-After` se mantienen. La nueva tasa será aceptada como
perfil productivo sólo después de completar `TASK-PROD-0001` durante una hora
con al menos 99.5% de respuestas durables, sin `202` sostenidos y con cola,
outbox, memoria y drenado acotados. 30/s queda como escalón de diagnóstico,
no como capacidad productiva.

### DEC-0073 — Perfil productivo reducido a 20/s tras ventana de 25/s

- Fecha: 2026-08-12
- Estado: `replaced`
- Reemplaza: el perfil operativo de `DEC-0072`
- Relacionado con: `TASK-PROD-0001`, `DEC-0071`

La ventana controlada de 25/s durante 15 minutos con 200 clientes produjo
1,292 respuestas `PENDING` frente a 21,564 `COMMITTED` (aproximadamente
5.7%). Las escrituras terminaron durables y SQLite pasó integridad, pero el
perfil no cumple el contrato de confirmación operativa. Una ventana de cinco
minutos a 20/s produjo 6,001/6,001 respuestas `200`, cero `202`/`429`, cliente
no saturado y p99 de 122.5 ms.

El valor predeterminado y el perfil recomendado se fijaron inicialmente en
`MAX_WRITES_PER_WINDOW=20` por store. La medición posterior de 20/s con
claves nuevas no cumplió el margen de confirmación, por lo que esta decisión
fue reemplazada por DEC-0074. El soak de una hora de
`TASK-PROD-0001` contra el SHA publicado sigue siendo obligatorio para cerrar
la validación de producción; 25/s y 30/s quedan como escalones de diagnóstico.

### DEC-0074 — Perfil productivo reducido a 15/s tras medición válida de 20/s

- Fecha: 2026-08-12
- Estado: `accepted`
- Reemplaza: `DEC-0073`
- Relacionado con: `TASK-PROD-0001`, `TASK-VAL-0020`

Una corrida limpia de cinco minutos a 20/s con 200 clientes, sobre el SHA
`a528af5`, produjo 5,056 respuestas `200` y 945 `202` de 6,001 operaciones
(84.25% confirmadas dentro del timeout de 2 segundos). Las 6,001 claves
terminaron en `_idempotency`, SQLite pasó `integrity_check` y el outbox quedó
en cero: el defecto observado es margen de latencia, no pérdida de datos.

Una corrida limpia de cinco minutos a 15/s, con prefijo de claves único y el
arnés corregido para aislar el warmup, sobre `1794268`, produjo 4,501/4,501
respuestas `200`, cero `202`/`429`, p99 de 62.09 ms, cero saturación del
generador y outbox en cero. Por ello el límite predeterminado y perfil
recomendado se fijan en `MAX_WRITES_PER_WINDOW=15` por store. La hora completa
contra el SHA final sigue siendo obligatoria antes de cerrar
`TASK-PROD-0001`; 20/s queda como diagnóstico y no como SLO.

### DEC-0071 — Admisión a tasa estable mediante token bucket

- Fecha: 2026-08-12
- Estado: `accepted`
- Relacionado con: `TASK-PROD-0001`, `TASK-VAL-0020`, `DEC-0070`
- Implementación: `crates/ixmati-api/src/throttle.rs`

La admisión por store usa un token bucket con la tasa configurada por
`MAX_WRITES_PER_WINDOW`/`THROTTLE_WINDOW_SECS` y una ráfaga máxima de tres
solicitudes. Esto conserva el límite medio de escrituras durables, pero evita
rechazar un cliente que opera a la tasa nominal por unos milisegundos de
jitter del planificador o de la red. REST devuelve `429` con `Retry-After` y
gRPC devuelve `RESOURCE_EXHAUSTED` con la misma indicación cuando el bucket
está vacío.

La ráfaga es deliberadamente pequeña: no pretende convertir el store de
escritor único en un sistema de ingestión ilimitada ni ocultar la saturación.
El soak de una hora de `TASK-PROD-0001` sigue siendo obligatorio para validar
que el perfil recomendado conserva latencia, memoria, cola y durabilidad
estables bajo tráfico realista.

### DEC-0075 — Batching productivo y cache post-commit

- Fecha: 2026-08-12
- Estado: `accepted`
- Relacionado con: `TASK-PROD-0001`, `TASK-VAL-0025`
- Implementación: `crates/ixmati-writer/src/main.rs`,
  `crates/ixmati-writer/src/cache_sync.rs`

La validación limpia de 15 escrituras durables/s mostró que el perfil anterior
dejaba al writer procesando una operación por transacción y sin margen: durante
la corrida de una hora en preparación aparecieron respuestas `PENDING` aunque
SQLite finalmente conservó las escrituras. El perfil de despliegue usa ahora
`BATCH_INTERVAL_MS=100`, que agrupa el tráfico nominal de 15/s en lotes
pequeños, y mantiene `BATCH_SIZE=100` como límite superior.

La cache base dejó de ejecutarse en el loop único del writer. Después de
confirmar SQLite y el ACK MQTT, los eventos se entregan a un worker post-commit
con cola acotada (`CACHE_SYNC_QUEUE_CAPACITY=1000`). La cache es una proyección
reconstruible; una cola llena no puede perder una escritura durable, y se
expone `cache_sync_deferred` para activar reconciliación/backpressure. También
se corrigió la conversión de eventos de eliminación para invalidar la entrada
en vez de volver a poblarla.

Una corrida limpia de cinco minutos sobre el árbol con estos cambios, a 15/s,
200 clientes y generador no saturado, produjo 4,501/4,501 respuestas `200`,
`p99=92.21 ms`, `max=152.47 ms`, `0` errores, `0` pendientes al finalizar,
`integrity_check=ok` y outbox drenado. Este resultado valida el mecanismo y el
perfil corto, pero no reemplaza el soak de una hora requerido por
`TASK-PROD-0001`.

### DEC-0076 — Recalibración del perfil productivo a 10/s por margen de confirmación

- Fecha: 2026-08-13
- Estado: `accepted`
- Reemplaza: `DEC-0074` para el límite operativo recomendado
- Relacionado con: `TASK-PROD-0001`, `TASK-VAL-0020`

La corrida prolongada del candidato de 15/s sobre `6fc80ab` no fue sostenible:
en aproximadamente ocho minutos acumuló 448 respuestas `202/PENDING` de 7,296
solicitudes observadas, aunque todas las claves terminaron comprometidas en
`_idempotency`. SQLite permaneció íntegro y no hubo pérdida; el fallo fue de
margen de latencia frente al timeout durable de 2 s.

Un control de cinco minutos a 10/s, con 200 clientes y generador no saturado,
produjo 3,001/3,001 respuestas `200`, p99 de 212.75 ms, cero errores, outbox
drenado e integridad correcta. Por tanto, `MAX_WRITES_PER_WINDOW=10` queda
como perfil recomendado provisional, mientras 15/s queda como diagnóstico.
La prueba de una hora contra el SHA recalibrado sigue siendo obligatoria antes
de cerrar `TASK-PROD-0001`; este cambio no convierte el control corto en una
garantía de disponibilidad.

### DEC-0077 — El perfil de 10/s requiere corregir la segunda ruta de escritura SQLite

- Fecha: 2026-08-13
- Estado: `accepted`
- Reemplaza: la conclusión de capacidad de `DEC-0076`, sin revertir su límite
  provisional de admisión
- Relacionado con: `TASK-PROD-0001`, `TASK-VAL-0025`

La primera ejecución exacta del SHA `b023819` a 10/s se detuvo después de
aproximadamente 12 minutos al observar 283 respuestas `202/PENDING`. El
writer tuvo pausas de hasta aproximadamente 1.25 s y luego procesó lotes
acumulados de hasta 20 comandos, mientras Mosquitto no reportó una
desconexión equivalente. SQLite terminó íntegra y el outbox drenó después del
alto, pero el criterio de confirmación durable no se cumplió.

La causa de diseño identificada es que `EventPublisher` abría una segunda
conexión SQLite para ejecutar `mark_published_batch`, en paralelo con la
conexión exclusiva del hilo de escritura. Aunque ambas operaciones eran
válidas individualmente, esto violaba la propiedad single-writer y podía
introducir contención en el mismo archivo durante carga sostenida.

La corrección consiste en enviar las marcas de `published_at` como trabajos al
`WriteHandle`; sólo ese hilo ejecuta escrituras SQLite. El cambio tiene prueba
unitaria específica y fue validado en amd64 con `6c38eb8`: una hora a 10/s
produjo `36,001/36,001` respuestas `200`, p99 de 212.86 ms, cero saturación
del cliente, cero outbox pendiente tras cinco minutos de drenado e
`integrity_check=ok`. Por tanto, 10/s queda como perfil productivo
demostrado para este host y configuración. La evaluación independiente de
150/s/200/s sigue pendiente y no debe extrapolarse desde este resultado.

### DEC-0078 — Artefactos amd64 y Litestream nativo con restore local verificable

- Fecha: 2026-08-13
- Estado: `accepted`
- Relacionado con: `TASK-WRITE-0014`, `TASK-STORE-0001`..`0006`,
  `TASK-INST-0006`
- Implementación: `helpers/make/artifacts.mk`, `helpers/make/dist-validate.mk`,
  `helpers/python/installer.py`, `systemd/ixmati-litestream-*.service`

La distribución `linux/amd64` se construye exclusivamente mediante el builder
Podman Linux y `make dist` recompila/extrae los binarios del checkout actual;
no reutiliza el `target/release` del host. `dist-validate` comprueba que cada
binario empaquetado sea ELF 64-bit x86-64. Esto evita publicar por accidente un
Mach-O/arm64 generado en macOS bajo el nombre de un artefacto Debian amd64.

La instalación nativa fija Litestream `0.5.16` y verifica el SHA256 del release
antes de instalarlo. Litestream v0.5 usa una réplica por proceso: el instalador
arranca `ixmati-litestream-file.service` para el watcher de directorio y sólo
configura/arranca `ixmati-litestream-s3.service` cuando existe
`IXMATI_LITESTREAM_S3_BUCKET`. La configuración y el entorno de credenciales
se conservan durante reinstalaciones. El restore local desde
`/var/lib/ixmati/backups` e `integrity_check` están validados en Debian; esto no
se considera evidencia de restore remoto, segundo destino ni RPO/RTO.

La suite de migración offline detiene ambas unidades Litestream antes de
checkpoint, crea backups locales, preserva tombstones, cuantifica conflictos,
deduplica idempotencias con digest idéntico y exige checksums reproducibles de
split. El E2E Debian valida además reconciler, cache, integridad y outbox.

Consecuencias: (+) el artefacto publicado es verificablemente ejecutable en el
target declarado; (+) una instalación limpia tiene backup local operable y
restaurable; (+) merge/split es auditable y determinista; (+) la ruta
S3-compatible queda cubierta por un smoke test con MinIO fijado por digest;
(-) el perfil de producción sigue limitado a la evidencia de capacidad
disponible; (-) un bucket remoto real, segundo destino, RPO/RTO y
cutover/rollback de routing siguen siendo gates pendientes y no se deben
presentar como resueltos.

### DEC-0079 — El ACK durable no retiene hilos de `spawn_blocking`

- Fecha: 2026-08-13
- Estado: `accepted`
- Relacionado con: `TASK-VAL-0025`, `TASK-PROD-0001`
- Implementación: `crates/ixmati-api/src/rest.rs`

El camino `ack_mode=committed` conserva la semántica durable: sólo responde
`200` después de encontrar la clave en `_idempotency` y responde `202 PENDING`
si vence la ventana configurada. La espera se implementa como sondeos cortos:
cada consulta SQLite ocupa un hilo de `spawn_blocking` sólo durante la lectura;
el intervalo entre sondeos se espera en Tokio.

La implementación anterior mantenía un hilo de `spawn_blocking` por petición
durante toda la ventana de hasta dos segundos. Con cientos de clientes eso
podía agotar el pool bloqueante del API y producir `PENDING` aunque el writer,
SQLite y sus colas siguieran progresando. El cambio reduce esa saturación
artificial sin convertir el ACK en optimista ni introducir un modo async nuevo.

Consecuencias: (+) el límite de concurrencia del API deja de estar atado a un
hilo bloqueante por ACK pendiente; (+) REST y gRPC comparten la corrección al
usar el mismo camino durable; (-) cada sondeo abre una conexión de lectura y
su impacto debe medirse nuevamente en Debian; (-) la capacidad productiva no
se amplía hasta que una prueba prolongada del SHA que contiene este cambio lo
demuestre.

### DEC-0080 — Límites explícitos de recursos en REST y gRPC

- Fecha: 2026-08-13
- Estado: `accepted`
- Relacionado con: `SPEC-PROTOBUF-0001`, `TASK-PROTO-0004`, producción segura
- Implementación: `crates/ixmati-api/src/rest.rs`,
  `crates/ixmati-api/src/grpc.rs`

REST aplica `MAX_REQUEST_BODY_BYTES` con un valor predeterminado de 1 MiB a
JSON y Protobuf. gRPC aplica `GRPC_MAX_MESSAGE_BYTES` de 1 MiB y
`GRPC_MAX_CONCURRENT_STREAMS` de 256 por conexión. El stream de eventos usa un
buffer acotado configurable (`EVENT_STREAM_BUFFER_CAPACITY`, 128 por defecto)
y reserva un slot para que un consumidor lento reciba `RESOURCE_EXHAUSTED` con
el cursor de reanudación; antes el canal podía llenarse y descartar ese error,
dejando un EOF silencioso.

Estos límites son controles de seguridad y aislamiento de recursos, no una
promesa de throughput. Se pueden ampliar explícitamente para un workload
conocido, pero no se deshabilitan implícitamente en una instalación nueva.
Los tests de API cubren el rechazo REST con 413 y el cierre del stream lento
con cursor; la capacidad durable sigue dependiendo del perfil del writer y de
la validación prolongada correspondiente.

### DEC-0081 — Las API keys scoped protegen todas las rutas de datos

- Fecha: 2026-08-13
- Estado: `accepted`
- Relacionado con: `SPEC-AUTH-0001`, `SPEC-PROTOBUF-0001`, `TASK-PROD-0003`
- Implementación: `crates/ixmati-api/src/auth/`, `crates/ixmati-api/src/rest.rs`,
  `crates/ixmati-api/src/grpc.rs`

Una identidad autenticada conserva el alcance declarado en
`IXMATI_API_KEY_SCOPES`, con formato `clave=store1|store2` y entradas separadas
por `;`. El alcance se aplica de forma uniforme a escritura, lectura,
consulta de estado y suscripción de eventos. Una clave sin alcance explícito
mantiene compatibilidad legacy y acceso global; en despliegues multi-tenant se
deben declarar todas las claves con alcance.

`/health`, `/ready` y `/metrics` siguen siendo endpoints operativos públicos.
Cuando la autenticación está habilitada, `/write`, `/read` y
`/writes/{store}/{idempotency_key}` requieren credenciales. Una clave scoped no
puede consultar una proyección sin store explícito porque esa consulta no tiene
un límite de aislamiento demostrable. El rechazo es HTTP 403 o
`PERMISSION_DENIED` en gRPC/Protobuf.

El instalador persiste `IXMATI_API_KEY_SCOPES` junto con `IXMATI_API_KEYS` en
`/etc/ixmati/ixmati.env`; omitirlo durante la instalación no debe convertir una
configuración scoped en global después de un reinicio.

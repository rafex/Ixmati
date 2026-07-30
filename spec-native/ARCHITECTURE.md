# ARCHITECTURE.md

Describe la arquitectura actual del proyecto y las opciones en evaluación.

## Visión general

Ixmati es un motor de escritura serializada que permite a múltiples backends/pods escribir en una misma instancia de SQLite sin contención. Los backends nunca abren SQLite directamente en modo escritura. Todas las escrituras pasan por un canal de ingesta (API REST/gRPC o MQTT) y son procesadas secuencialmente por un único writer Rust. Las lecturas se sirven desde una caché (FlashDB) con fallback directo a SQLite. Litestream replica el WAL a destinos remotos para disaster recovery.

## Opciones de arquitectura (abiertas)

Ambas opciones comparten el mismo objetivo y comparten SQLite como fuente de verdad con Litestream como backup. Difieren en el canal de ingesta de escrituras y en el camino de lectura.

### Opción A — Cola de escritura (Mosquitto como buffer)

```
                       ┌─────────────┐
   [Backend 1] ───────►│             │
   [Backend 2] ───────►│  API Layer  │────► Mosquitto ────► Writer ────► SQLite
   [Backend N] ───────►│ (REST/gRPC) │       (persistent,       │
                       └─────────────┘        QoS 1)            ▼
                                                          Litestream ──► VPS Backup
                                                               │
                                                               ▼
                                                          FlashDB ◄──── Lecturas directas
                                                               ▲
   [Backend] ──── lectura ────► FlashDB (cache)               │
                                │ miss                        │
                                └────► SQLite ──── repoblar ──┘
```

**Escritura**: el backend publica en un topic MQTT o llama a la API REST/gRPC, que publica en Mosquitto. El writer consume mensajes de Mosquitto y escribe en SQLite. Tras cada commit, el writer actualiza FlashDB (invalidación o repoblación).

**Lectura**: el backend consulta FlashDB. Si la clave no existe (miss), consulta SQLite directamente, guarda el resultado en FlashDB con TTL, y lo retorna.

### Opción B — FlashDB como buffer de ingesta

```
                       ┌─────────────┐
   [Backend 1] ───────►│             │
   [Backend 2] ───────►│  API Layer  │────► FlashDB ──────► Worker ────► SQLite
   [Backend N] ───────►│ (REST/gRPC) │     (buffer de            │
                       └─────────────┘      escritura)           ▼
                                                           Litestream ──► VPS Backup
                                                                │
   [Backend] ──── lectura ────► API ────► Mosquitto ────► Worker ────► SQLite
                                             (cola de                    │
                                              lecturas)                  ▼
                                                                   FlashDB ◄── materialización
                                                                        │
                                                                        └────► retorno al backend
```

**Escritura**: el backend escribe directamente en FlashDB (ingesta rápida, sin bloqueos). Un worker Rust lee de FlashDB y aplica los cambios a SQLite de forma serializada.

**Lectura**: el backend envía una petición de lectura a la API, que la encola en Mosquitto. Un worker consume la cola, consulta SQLite, materializa el resultado en FlashDB, y lo retorna al backend.

### Tabla comparativa

| Aspecto | Opción A (Mosquitto buffer) | Opción B (FlashDB buffer) |
|---|---|---|
| **Durabilidad de escrituras en tránsito** | Alta: Mosquitto con persistence + QoS 1 garantiza que no se pierden | Baja: FlashDB no está diseñado como buffer transaccional; pérdida si el worker cae antes de leer |
| **Latencia de ack de escritura** | Baja: publicación en MQTT o escritura en API | Muy baja: escritura local en FlashDB |
| **Garantía de orden** | Por topic particionado (entidad+id) | Depende de implementación; sin FIFO nativo |
| **Read-your-writes** | Garantizado en modo sync | No garantizado: el worker puede no haber aplicado aún la escritura |
| **Carga en SQLite** | Solo escrituras del writer + fallback de lecturas | Escrituras del worker + cada lectura (vía cola) genera una consulta |
| **Complejidad operativa** | Media: mantener Mosquitto + writer + FlashDB | Alta: dos workers (escritura y lectura), doble almacenamiento, resync bidireccional |
| **Modos de fallo** | Si Mosquitto cae, se rechazan escrituras (fail-stop). Los mensajes persisten en disco. | Si FlashDB cae, se pierden escrituras no aplicadas. Si el worker cae, hay backlog no leído. |
| **Explosión de datos en FlashDB** | Solo claves cacheadas con TTL | Cada consulta de lectura distinta crea una entrada; riesgo de crecimiento no acotado |

### Criterios de decisión

La decisión final entre Opción A y Opción B debe basarse en evidencia experimental, no en preferencia:

1. **Durabilidad de FlashDB ante crash**: escribir en FlashDB, matar el worker con `kill -9`, reiniciar. ¿Cuántos registros se perdieron?
2. **Latencia de escritura extremo a extremo**: medir p50/p99 desde que el backend publica/escribe hasta que el dato está en SQLite, para ambas opciones.
3. **Orden bajo carga concurrente**: 10 backends escribiendo sobre el mismo `entity_id` en paralelo. ¿Se preserva el orden en SQLite?
4. **Costo de resync**: reconstruir FlashDB desde SQLite. Tiempo y carga para 100k, 1M, 10M registros.
5. **Complejidad del código**: líneas de Rust necesarias para implementar cada opción. Mantenibilidad a 6 meses.

La opción que se elija se registrará como `DEC-0010` y `DEC-0011` en estado `accepted`.

## Módulos principales

Independientemente de la opción elegida, el sistema se compone de estos módulos:

- **API Gateway** (`api-gateway`): expone endpoints REST y gRPC para escritura y lectura. Traduce requests a mensajes internos. Responsable de validación, rate limiting, y enrutamiento entre modo async y sync.
- **Writer** (`writer`): único proceso que abre SQLite en modo escritura. Consume mensajes del canal de ingesta (Mosquitto o FlashDB), aplica escrituras con `BEGIN IMMEDIATE`, batching, y deduplicación por `idempotency_key`.
- **Cache Layer** (`cache-layer`): abstracción sobre FlashDB. Expone operaciones de get/set/invalidate con TTL. Usado por el writer para invalidar/repoblar tras escrituras, y por la API de lectura para servir consultas.
- **Resync Module** (`resync`): comando offline que reconstruye FlashDB desde SQLite. Necesario para bootstrap inicial y recuperación tras corrupción de cache.
- **Observability** (`observability`): métricas (lag de cola, latencia de commit, tasa de misses, tamaño de cache) y health checks (writer vivo, Mosquitto conectado, FlashDB responde).

## Restricciones

- **Dependencias prohibidas**:
  - Ningún módulo distinto del writer abre SQLite en modo escritura.
  - Los backends externos no acceden directamente a SQLite ni a FlashDB.
  - La API de lectura no debe depender del canal de escritura (no acoplar lectura y escritura).
- **Acoplamientos a evitar**:
  - El formato de envelope de mensaje no debe estar acoplado a la implementación interna del writer.
  - La semántica de sync/async no debe filtrarse a la capa de storage (SQLite no sabe de modos).
- **Límites de infraestructura**:
  - SQLite en WAL con un solo archivo de base de datos (no sharding).
  - FlashDB como cache volátil; siempre reconstruible desde SQLite.
  - Mosquitto con persistence en disco; sin clustering (single broker).
  - Un solo writer activo; failover manual u orquestado externamente.

## Riesgos

| Riesgo | Impacto | Mitigación |
|---|---|---|
| Caída del writer | Escrituras bloqueadas hasta reinicio | Mosquitto retiene mensajes en disco; health check + alerta; posible writer standby (futuro) |
| Inconsistencia FlashDB | Lecturas devuelven datos obsoletos | TTL corto + invalidación por el writer + comando de resync. Lectura con fallback a SQLite. |
| Pérdida de mensajes en Mosquitto | Escrituras descartadas | `persistence true` + QoS 1 + `autosave_interval`. Validar con test de crash. |
| Corrupción de SQLite | Pérdida de datos canónicos | Litestream a ≥2 destinos + backups regulares. PRAGMA integrity_check periódico. |
| FlashDB sin binding Rust maduro | Bloquea la capa de cache | Criterio de salida: evaluar sled, redb, o lmdb-rs como alternativas si el binding FFI no es viable. |
| Explosión de cache | FlashDB sin límite de tamaño | TTL obligatorio + max keys + LRU eviction si el binding lo soporta. Resync completo como válvula de escape. |

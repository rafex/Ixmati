# ARCHITECTURE.md

Describe la arquitectura actual del proyecto.

## Visión general

Ixmati es un motor de escritura serializada que permite a múltiples backends/pods escribir en una misma instancia de SQLite sin contención. La unidad atómica del sistema es el **store**: un archivo SQLite con su propio writer, su propio prefijo de topic y su propio destino de backup. Con 1 store se obtiene el diseño monolítico original. Con N stores nombrados por bounded context, tenant o región se obtiene la variante DDD.

Los backends nunca abren SQLite directamente. Todas las escrituras pasan como comandos por la API (REST/gRPC), se publican en Mosquitto, y son consumidas por el writer del store correspondiente. Las lecturas se sirven desde FlashDB: cache-aside por defecto (`c:<store>:<entity>:<key>`) y proyecciones opt-in (`p:<projection>:<key>`) alimentadas por un bus de eventos.

Litestream replica el WAL de cada store a destinos remotos. El sistema es tolerante a crash del writer (outbox transaccional garantiza 0 eventos perdidos) y a corrupción de cache (reconstruible desde los stores vía reconciler).

## Arquitectura (decisión cerrada)

```
                          ┌──────────────────────────────┐
                          │         API Layer            │
                          │     (REST/gRPC + OpenAPI)    │
                          └──────┬──────────────┬────────┘
                                 │              │
                         escritura              lectura
                                 │              │
                                 ▼              ▼
                          ┌──────────────┐ ┌──────────────┐
                          │  Mosquitto   │ │   FlashDB    │
                          │ (persistent, │ │  (cache +    │
                          │  QoS 1)      │ │  read models)│
                          └──────┬───────┘ └──────┬───────┘
                                 │                │
                    ┌────────────┼────────┐       │ miss
                    │            │        │       │
                    ▼            ▼        ▼       ▼
              ┌──────────┐ ┌──────────┐ ┌──────────┐
              │ Writer   │ │ Writer   │ │ Writer   │  ← 1 proceso por store
              │ store A  │ │ store B  │ │ store N  │
              └────┬─────┘ └────┬─────┘ └────┬─────┘
                   │            │            │
              ┌────┴────┐  ┌───┴────┐  ┌───┴────┐
              │SQLite A │  │SQLite B│  │SQLite N│  ← fuente de verdad por store
              └────┬────┘  └───┬────┘  └───┬────┘
                   │            │            │
              ┌────┴────┐  ┌───┴────┐  ┌───┴────┐
              │Litestrm │  │Litestrm│  │Litestrm│  ← backup por store
              └────┬────┘  └───┬────┘  └───┬────┘
                   │            │            │
                   ▼            ▼            ▼
              ┌──────────────────────────────────────┐
              │    S3 / VPS Backup (≥2 destinos)     │
              └──────────────────────────────────────┘

  Flujo de proyección (bus de eventos, separado de comandos):

  Writer(store) ──► _outbox ──► publicador ──► ixmati/evt/...
                                                   │
                              ┌────────────────────┼──────────────┐
                              ▼                    ▼              ▼
                        Projector A          Projector B    (auditoría,
                         (read model)         (read model)   monitoreo)
                              │                    │
                              ▼                    ▼
                           FlashDB             FlashDB
                        p:proj_a:*           p:proj_b:*
```

## Flujo de escritura

### Interfaces binarias

`ixmati-api` mantiene REST/JSON en `API_PORT` y sirve gRPC en un listener
separado `GRPC_HOST:GRPC_PORT` (por defecto `0.0.0.0:30100` en despliegues).
REST puede negociar `application/protobuf`. Ambas rutas convierten al mismo
`WriteEnvelope`, `ReadRequest` y estado durable; no existe un camino de
escritura async distinto. El payload Protobuf es `google.protobuf.Struct` y el
campo de bytes histórico sólo se conserva deprecated.

La autenticación gRPC usa metadata `x-api-key` y el mismo conjunto
`IXMATI_API_KEYS` de REST. `EventService.SubscribeEvents` usa el `id` de
`_outbox` como cursor, reproduce la retención disponible y continúa con live
en entrega at-least-once. No se habilitan reflection ni el health protocol
estándar.

1. El backend envía un comando a la API (`POST /write` o gRPC `Write`).
2. La API valida el envelope (store obligatorio, `idempotency_key`, `version`, `ack_mode`).
3. La API publica el comando en `ixmati/cmd/<store>/<entity>/<id>` (Mosquitto, QoS 1).
4. `ack_mode=accepted` se conserva como alias de compatibilidad, pero la API
   sólo devuelve `200` después de que `_idempotency` confirme el commit
   SQLite. Si aún no puede confirmarlo devuelve `202 PENDING` con la
   `idempotency_key`; en multi-store la consulta usa `SQLITE_PATHS`.
5. Si `ack_mode=committed`: la API consulta `_idempotency` hasta confirmar el
   commit o agotar `WRITE_COMMITTED_TIMEOUT_MS`; esa espera se ejecuta en el
   blocking pool y reutiliza una conexión SQLite por espera, para no bloquear
   los workers async del API. Un timeout devuelve `202 PENDING` y deja la
   consulta disponible en `GET /writes/...`.
6. El writer del store consume el comando, lo acumula en un batch.
7. El batch se vacía al cumplirse `MAX_BATCH_SIZE` o `MAX_BATCH_INTERVAL_MS`;
   el perfil productivo actual usa 100 ms para agrupar escrituras a 10/s,
   comprobando el intervalo también cuando siguen llegando comandos; así el
   tráfico continuo no puede esperar indefinidamente a llenar el batch.
   Entonces ejecuta `BEGIN IMMEDIATE`:
   - Aplica el comando a la tabla de la entidad.
   - Inserta en `_idempotency`.
   - Inserta el evento en `_outbox` (misma transacción).
   - `COMMIT`.
8. El publicador (task interna del writer) lee `_outbox WHERE published_at IS NULL`, publica en `ixmati/evt/<store>/<entity>/<id>`, y marca `published_at`.
9. El writer confirma el consumo MQTT después del commit SQLite. El publicador
   marca `_outbox.published_at` sólo después de recibir `PUBACK`; la entrega de
   eventos es at-least-once.
10. La sincronización de la cache base se encola después del commit en un worker
    post-commit acotado (`CACHE_SYNC_QUEUE_CAPACITY`). Una cache lenta no bloquea
    al writer; si la cola se llena, el evento sigue siendo recuperable desde
    `_outbox` y el diferido queda expuesto en métricas para reconciliación.
11. Los proyectores reciben el evento y actualizan sus read models en FlashDB de forma idempotente.

## Flujo de lectura

1. El backend consulta la API (`GET /read` o gRPC `Read`).
2. Si especifica `projection`, la API busca en `p:<projection>:<key>` en FlashDB.
3. Si especifica `store` + `entity` + `key`, la API busca en `c:<store>:<entity>:<key>` (cache-aside).
4. Si hay hit: se devuelve el dato. Fin.
5. Si hay miss: la API consulta SQLite del store en modo solo lectura.
6. Si existe en SQLite: se guarda en FlashDB (cache-aside) con TTL, y se devuelve.
7. Si no existe en SQLite: 404.

## Módulos principales

| Módulo | Crate | Responsabilidad |
|---|---|---|
| **API Gateway** | `ixmati-api` | Endpoints REST + gRPC. Validación, rate limiting, enrutamiento async/sync, consulta de estado. Traduce requests a comandos (JSON) y los publica en Mosquitto. |
| **Core** | `ixmati-core` | Tipos compartidos: `WriteEnvelope`, `EventEnvelope`, `AckResponse`, `WriteStatus`, `Error`, `Config`, `StoreConfig`. Trait `CacheBackend`. Lógica de proyección compartida con reconciler. |
| **Writer** | `ixmati-writer` | Único proceso que abre SQLite en modo escritura. Consume comandos de `ixmati/cmd/...`, aplica batches con `BEGIN IMMEDIATE`, deduplicación, outbox transaccional. Incluye el publicador de eventos como task interna. Un writer por store. |
| **Cache** | `ixmati-cache` | Abstracción sobre FlashDB (o alternativa). Expone `get`/`set`/`invalidate` con TTL. Namespaces `c:` y `p:`. |
| **Projector** | `ixmati-projector` | Consume eventos de `ixmati/evt/...`. Aplica proyecciones declaradas en config (patrón R o M). Idempotente por `event_id` o por upsert natural. |
| **Supervisor** | `ixmati-supervisor` | Orquesta múltiples stores en un solo proceso (single-VPS) o gestiona el ciclo de vida de pods (K8s). Config de topología. |
| **Reconciler** | `ixmati-reconciler` | Binario offline. Reconstruye read models (`p:*`) desde los stores fuente en modo solo lectura. Fan-in sobre N stores. Soporta reproyección total y selectiva. |

## Invariantes

1. **Un writer por store**: solo el writer asignado al store abre ese archivo SQLite en modo escritura. Nunca 2 pods sobre el mismo store.
2. **Un comando, un store**: el campo `store` es obligatorio en todo comando. Un comando no puede modificar 2 stores.
3. **Ningún proceso con 2 stores en write**: un writer solo escribe en su store asignado.
4. **Outbox atómico**: el evento se escribe en `_outbox` dentro de la misma transacción que los datos. No hay publicación de eventos fuera de la transacción.
5. **Publicador interno**: la task que publica eventos desde `_outbox` es parte del proceso writer, no un proceso separado.
6. **Proyectores idempotentes**: la re-entrega de un evento (QoS 1) no produce duplicados en los read models.
7. **ATTACH solo lectura**: `ATTACH DATABASE` está prohibido en cualquier conexión con permisos de escritura.
8. **Cache desechable**: tanto cache-aside como read models son reconstruibles desde los stores. Borrar FlashDB no pierde datos.

## Restricciones

- **Dependencias prohibidas**:
  - Ningún módulo distinto del writer de un store abre ese SQLite en modo escritura.
  - `ixmati-api` no depende de `ixmati-writer` ni de `ixmati-projector` (solo conoce `ixmati-core` y Mosquitto).
  - `ixmati-projector` no abre SQLite en escritura.
- **Acoplamientos a evitar**:
  - El formato de envelope de comando no debe estar acoplado a la implementación interna del writer.
  - Los proyectores no deben conocer la estructura interna de los stores fuente (consume eventos, no tablas).
- **Límites de infraestructura**:
  - Sin JOIN SQL cross-store operacional (solo ATTACH read-only para analítica).
  - Sin transacción cross-store (sagas en la aplicación).
  - Mosquitto single broker (sin clustering).

## Riesgos

| Riesgo | Impacto | Mitigación |
|---|---|---|
| Caída del writer de un store | Escrituras bloqueadas para ese store | Mosquitto retiene mensajes en disco. Health check + alerta. Los demás stores no se ven afectados. |
| Evento perdido (dual-write) | Read models desincronizados sin error visible | Outbox transaccional (`_outbox` en la misma tx). 0 eventos perdidos por diseño. |
| Lag de proyección | Read models devuelven datos obsoletos | Métrica de lag por proyección. TTL corto en FlashDB como safety net. |
| Fan-out descontrolado en patrón M | Un cambio dispara N rewrites en FlashDB | Regla de decisión explícita en DEC-0016 (fan_out ≤ 100). Violación detectada en validación de proyección. |
| Store caliente | Un store concentra el 90% del tráfico y se convierte en cuello de botella | El sharding por store no es sharding por carga. Si ocurre, re-evaluar partición interna o migrar ese store a otro motor. |
| Inconsistencia FlashDB | Lecturas devuelven datos obsoletos | TTL + invalidación por writer + fallback a SQLite. Reconciler como último recurso. |
| Corrupción de SQLite | Pérdida de datos canónicos | Litestream a ≥2 destinos por store. PRAGMA integrity_check periódico. |
| N× coste operativo | N stores = N writers, N Litestream, N PVCs | Aceptado como trade-off del aislamiento. Optimizable con supervisor single-process en entornos pequeños. |
| Renombrar un store | Equivale a migración de base de datos | Stores estables por diseño. Si es inevitable: nuevo store + migración de datos + redirección de tráfico + decomiso del viejo. |

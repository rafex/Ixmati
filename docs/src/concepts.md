## Conceptos

### Store

Unidad atómica del sistema. Un store es la tupla `(archivo SQLite, un writer, un prefijo de topic, un destino Litestream)`. Es la unidad de serialización, transaccionalidad, backup y aislamiento de fallo. El motor funciona con `stores=1` (sin overhead de eventos) o `stores=N` (aislamiento por bounded context).

### Comando

Un mensaje de escritura enviado a la API y publicado en `ixmati/cmd/<store>/<entity>/<id>`. Incluye `op` (upsert/delete/patch), `store`, `version`, `idempotency_key` y `ack_mode`. Un comando toca exactamente 1 store. Se puede rechazar (`VERSION_CONFLICT`, `DUPLICATE`).

### Evento

Un hecho consumado publicado en `ixmati/evt/<store>/<entity>/<id>`. Se genera desde el outbox transaccional tras cada commit exitoso del writer. No se puede rechazar. Es consumido por proyectores para actualizar read models.

### Proyección

Un read model materializado en FlashDB bajo el namespace `p:<projection>:<key>`. Se actualiza de forma eager al recibir eventos. Se declara en config con patrón **R** (referencia + lookup, 2 lecturas sin fan-out) o **M** (materializado, 1 lectura pero con fan-out en escritura). Las proyecciones son opt-in y completamente reconstruibles desde los stores vía el reconciler.

### Outbox transaccional

El evento se inserta en la tabla `_outbox` dentro de la misma transacción `BEGIN IMMEDIATE` que los datos de la entidad. Esto garantiza que nunca se pierde un evento aunque el proceso muera entre el commit y la publicación.

### Cache-aside

Namespace `c:<store>:<entity>:<key>` en FlashDB. Se llena de forma lazy en cada read-miss desde SQLite. Se invalida o repuebla por el writer tras cada commit. Es la estrategia de lectura por defecto.

### Modos de confirmación

- **`accepted`** (alias durable): se acepta por compatibilidad, pero el `200`
  sólo llega después del commit; si no, el backend recibe `202` y puede
  consultar `GET /writes/{store}/{idempotency_key}`.
- **`committed`** (sync): ack solo tras commit exitoso en SQLite. Garantiza read-your-writes en ese store.

### Namespaces de FlashDB

- `c:<store>:<entity>:<key>` → cache-aside (lazy, TTL).
- `p:<projection>:<key>` → read models (eager, alimentados por eventos).
- Purgables por prefijo. `c:pedidos:*` no afecta `p:*` ni otros stores.

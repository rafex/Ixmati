### Envelope de Comando

| Campo | Tipo | Obligatorio | Descripción |
|---|---|---|---|
| `op` | string | Sí | upsert, delete, patch |
| `store` | string | Sí | Store destino. Un comando = 1 store |
| `entity` | string | Sí | Nombre de la entidad |
| `key` | string | Sí | Clave primaria |
| `version` | u64 | Sí | Monotónico. Se rechaza <= stored |
| `ts` | ISO 8601 | Sí | Timestamp origen |
| `idempotency_key` | UUID | Sí | Scope: (store, key) |
| `ack_mode` | string | Sí | accepted (alias durable) o committed |
| `payload` | object | Sí | Datos de la entidad |

### Envelope de Evento

| Campo | Tipo | Descripción |
|---|---|---|
| `event_id` | UUID | Identificador único. Idempotencia de proyectores |
| `event_type` | string | `<entity>.<accion>` (ej. pedido.creado) |
| `store` | string | Store donde ocurrió |
| `entity` | string | Entidad modificada |
| `key` | string | Identificador |
| `version` | u64 | Versión resultante |
| `occurred_at` | ISO 8601 | Timestamp del commit |
| `outbox_seq` | u64 | Secuencia del outbox |
| `payload` | object | Datos tras el cambio |

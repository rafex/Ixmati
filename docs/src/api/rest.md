### Endpoints REST

| Método | Ruta | Descripción |
|---|---|---|
| `POST` | `/write` | Enviar comando de escritura |
| `GET` | `/writes/{store}/{idempotency_key}` | Consultar estado de comando async |
| `GET` | `/read` | Leer por store/entity/key o por proyección |
| `GET` | `/health` | Health check agregado |

#### POST /write

```json
{
  "op": "upsert",
  "store": "pedidos",
  "entity": "pedido",
  "key": "ped_abc",
  "version": 1,
  "idempotency_key": "uuid",
  "ack_mode": "committed",
  "payload": {}
}
```

`ack_mode=accepted` se conserva como alias compatible, pero no cambia la
durabilidad: la API sólo devuelve `200 OK` después de confirmar el commit en
SQLite. Si el commit todavía no puede confirmarse dentro del timeout devuelve
`202 Accepted` con la `idempotency_key` para consultar `GET /writes/{store}/{idempotency_key}`.

En un despliegue multi-store, configurar `SQLITE_PATHS` como una lista
`store=/ruta/db,otro=/ruta/otra.db` para que cada confirmación consulte el
SQLite del writer correspondiente.

#### GET /writes/{store}/{idempotency_key}

```json
{"status": "applied", "applied_at": "2026-07-29T10:30:00Z"}
```

Estados: `pending`, `applied`, `rejected` (con error code).

#### GET /read

Parámetros: `store` + `entity` + `key` (cache-aside) o `projection` + `key` (read model).

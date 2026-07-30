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

Respuesta: `202 Accepted` con `write_id` si `ack_mode=accepted`. `200 OK` con resultado si `ack_mode=committed`.

#### GET /writes/{store}/{idempotency_key}

```json
{"status": "applied", "applied_at": "2026-07-29T10:30:00Z"}
```

Estados: `pending`, `applied`, `rejected` (con error code).

#### GET /read

Parámetros: `store` + `entity` + `key` (cache-aside) o `projection` + `key` (read model).

### Códigos de Error

| Código | HTTP | Descripción |
|---|---|---|
| `VERSION_CONFLICT` | 409 | version <= stored_version |
| `DUPLICATE` | 409 | idempotency_key ya procesada |
| `STORE_NOT_FOUND` | 404 | El store no existe |
| `ENTITY_NOT_FOUND` | 404 | La entidad no existe en el store |
| `WRITE_REJECTED` | 422 | Comando rechazado (validación de negocio) |
| `QUEUE_FULL` | 503 | Cola MQTT saturada |
| `PROJECTION_ERROR` | 500 | Error en proyector |
| `INTERNAL` | 500 | Error interno no recuperable |

### Formato de respuesta de error

```json
{
  "error": "VERSION_CONFLICT",
  "detail": "version 5 <= stored version 7",
  "store": "pedidos",
  "idempotency_key": "abc-def"
}
```

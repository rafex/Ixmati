# CONVENTIONS.md

Reglas de código, naming y estructura del proyecto.

## Layout del workspace Cargo

```
ixmati/
├── Cargo.toml              ← workspace root
├── crates/
│   ├── ixmati-api/         ← axum + tonic, endpoints REST/gRPC
│   ├── ixmati-writer/      ← lógica de escritura, único acceso write a SQLite
│   ├── ixmati-cache/       ← abstracción sobre FlashDB (o alternativa)
│   ├── ixmati-core/        ← tipos compartidos, envelope, errores, config
│   └── ixmati-resync/      ← binario de reconstrucción de cache
├── proto/                  ← archivos .proto para gRPC
├── docker/                 ← Dockerfiles y docker-compose
└── config/                 ← archivos de configuración (mosquitto.conf, etc.)
```

- Un crate por responsabilidad. `ixmati-core` es el único crate del que dependen todos los demás.
- Ningún crate depende de `ixmati-api` ni de `ixmati-writer` excepto el binario final.
- `ixmati-cache` expone un trait (`CacheBackend`) para facilitar el cambio de implementación (FlashDB → sled, etc.).

## Naming

| Elemento | Convención | Ejemplo |
|---|---|---|
| Crates | `ixmati-<dominio>` | `ixmati-writer` |
| Módulos | `snake_case` | `mod cache_layer` |
| Tipos públicos | `PascalCase` | `WriteEnvelope`, `CacheBackend` |
| Funciones | `snake_case` | `fn apply_batch()` |
| Constantes | `SCREAMING_SNAKE_CASE` | `MAX_BATCH_SIZE` |
| Errores | `thiserror` derive, `PascalCase` | `WriteError`, `CacheError` |
| Topics MQTT | `ixmati/<dominio>/<entidad>/<id>` | `ixmati/write/user/123` |

## Formato de topics MQTT

```
ixmati/write/<entity>/<id>     ← escrituras
ixmati/write/<entity>/<id>/ack ← confirmaciones de commit (modo sync)
ixmati/health/<component>       ← heartbeats
```

- `<entity>`: nombre de la entidad en snake_case (`user`, `order`, `session`).
- `<id>`: identificador único de la entidad (string o UUID).
- El particionado por `entity/id` garantiza que todos los mensajes para una misma entidad+id llegan al mismo consumidor, preservando orden.

## Envelope de mensaje

Todo mensaje de escritura (MQTT o API) sigue este esquema:

```json
{
  "op": "upsert",
  "entity": "user",
  "key": "usr_abc123",
  "version": 7,
  "ts": "2026-07-29T10:30:00Z",
  "idempotency_key": "uuid-v4",
  "ack_mode": "committed",
  "payload": {}
}
```

| Campo | Tipo | Obligatorio | Descripción |
|---|---|---|---|
| `op` | string enum | Sí | `upsert`, `delete`, `patch` |
| `entity` | string | Sí | Nombre de la entidad |
| `key` | string | Sí | Identificador único de la entidad (clave primaria) |
| `version` | u64 | Sí | Número de versión monotónico. El writer rechaza versiones menores o iguales a la almacenada. |
| `ts` | ISO 8601 | Sí | Timestamp del origen (no del writer). Para resolución de conflictos last-write-wins. |
| `idempotency_key` | UUID v4 | Sí | Clave de idempotencia. El writer registra las ya procesadas y rechaza duplicados. |
| `ack_mode` | string enum | Sí | `accepted` (async: ack inmediato de recepción) o `committed` (sync: ack tras commit en SQLite). |
| `payload` | object | Sí | Datos específicos de la entidad. Schema libre validado por el backend antes del envío. |

**Reglas de procesamiento**:
- El writer rechaza (ack con error) mensajes con `version <= stored_version`.
- La tabla `_idempotency` almacena `(idempotency_key, applied_at, status)`. TTL de limpieza: 24h.
- En modo `committed`, el writer publica la confirmación en `ixmati/write/<entity>/<id>/ack`.
- En modo `accepted`, el ack es inmediato al recibir el mensaje. El backend puede consultar `GET /writes/{idempotency_key}` para el estado.

## Errores

- Usar `thiserror` para errores de dominio.
- Los errores expuestos en la API siguen el formato:

```json
{
  "error": "VERSION_CONFLICT",
  "detail": "version 5 is not newer than stored version 7",
  "idempotency_key": "abc-def"
}
```

- Códigos de error estándar: `VERSION_CONFLICT`, `DUPLICATE`, `ENTITY_NOT_FOUND`, `WRITE_REJECTED`, `QUEUE_FULL`, `INTERNAL`.
- Nunca exponer stack traces ni detalles internos de SQLite en las respuestas de error.

## Logging estructurado

- Usar `tracing` con spans para trazabilidad extremo a extremo.
- Todo mensaje procesado por el writer debe loguear: `idempotency_key`, `entity`, `key`, `op`, `version`, `ack_mode`, `latency_ms`.
- Usar `tracing-subscriber` con formato JSON en producción.
- Niveles: `ERROR` (pérdida de datos, crash), `WARN` (rechazo, reintento), `INFO` (commit exitoso, batch aplicado), `DEBUG` (detalle de mensaje), `TRACE` (operaciones de cache).

## Tests

- Tests unitarios: `#[cfg(test)] mod tests` en el mismo archivo.
- Tests de integración: `tests/` en cada crate, usando SQLite en memoria (`:memory:`) y Mosquitto en Docker.
- Tests de crash: script aparte que lanza el writer, lo mata con `kill -9`, y verifica 0 pérdidas.
- Cobertura mínima objetivo: 80% en `ixmati-writer`, 70% en `ixmati-api`.

# CONVENTIONS.md

Reglas de código, naming y estructura del proyecto.

## Layout del workspace Cargo

```
ixmati/
├── Cargo.toml              ← workspace root
├── crates/
│   ├── ixmati-api/         ← axum + tonic, endpoints REST/gRPC
│   ├── ixmati-writer/      ← lógica de escritura, único acceso write a SQLite por store
│   ├── ixmati-cache/       ← abstracción sobre FlashDB (o alternativa)
│   ├── ixmati-projector/   ← consume eventos, actualiza read models en FlashDB
│   ├── ixmati-core/        ← tipos compartidos, envelope comando+evento, errores, config, StoreConfig
│   ├── ixmati-reconciler/  ← binario de reproyección offline (fan-in sobre N stores)
│   └── ixmati-supervisor/  ← orquesta múltiples stores (N pods o 1 proceso con N writers)
├── proto/                  ← archivos .proto para gRPC
├── docker/                 ← Dockerfiles y docker-compose
├── k8s/                    ← Kubernetes manifests (Deployment, PVC, sidecar por store)
└── config/                 ← archivos de configuración (mosquitto.conf, stores.toml, projections.toml)
```

- Un crate por responsabilidad. `ixmati-core` es el único crate del que dependen todos los demás.
- Ningún crate depende de `ixmati-api` ni de `ixmati-writer` excepto el binario final.
- `ixmati-cache` expone un trait (`CacheBackend`) para facilitar el cambio de implementación (FlashDB → sled, etc.).
- La lógica de proyección se comparte entre `ixmati-projector` (online) y `ixmati-reconciler` (offline) vía `ixmati-core`.

## Naming

| Elemento | Convención | Ejemplo |
|---|---|---|
| Crates | `ixmati-<dominio>` | `ixmati-projector` |
| Módulos | `snake_case` | `mod cache_layer` |
| Tipos públicos | `PascalCase` | `WriteEnvelope`, `EventEnvelope`, `StoreConfig` |
| Funciones | `snake_case` | `fn apply_batch()` |
| Constantes | `SCREAMING_SNAKE_CASE` | `MAX_BATCH_SIZE` |
| Errores | `thiserror` derive, `PascalCase` | `WriteError`, `ProjectionError` |
| Topics MQTT | `ixmati/<tipo>/<store>/<entity>/<id>` | `ixmati/cmd/pedidos/pedido/456` |

## Naming de stores

- `snake_case`, sin `/` ni caracteres especiales.
- Estable e inmutable: renombrar un store requiere crear uno nuevo, migrar datos, redirigir tráfico y decommissionar el viejo. No es una operación online.
- "Dominio" es una etiqueta de config (`label: "pedidos"`), no un concepto del código. El motor solo ve stores.

## Taxonomía de topics MQTT

### Comandos

```
ixmati/cmd/<store>/<entity>/<id>        ← escrituras
ixmati/cmd/<store>/<entity>/<id>/ack    ← confirmaciones de commit (modo sync)
```

- `<store>`: nombre del store destino. **Obligatorio**.
- `<entity>`: nombre de la entidad en snake_case.
- `<id>`: identificador único de la entidad (string o UUID).
- QoS 1, `retained false`.
- Consumidor: exactamente 1 (el writer del store).
- Un comando se puede rechazar (`VERSION_CONFLICT`, `DUPLICATE`).

### Eventos

```
ixmati/evt/<store>/<entity>/<id>        ← eventos de cambio
```

- Misma estructura de `<store>/<entity>/<id>` que los comandos.
- QoS 1, `retained false`.
- Consumidores: N (proyectores, auditoría, monitoreo).
- Un evento **no se puede rechazar** (es un hecho consumado).
- Retención mayor que comandos (permitir reproyección desde cero si es necesario).

### Health

```
ixmati/health/<store>                   ← heartbeats de writer por store
ixmati/health/api                       ← heartbeat de API
```

## Envelope de comando

```json
{
  "op": "upsert",
  "store": "pedidos",
  "entity": "pedido",
  "key": "ped_abc123",
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
| `store` | string | **Sí** | Store destino del comando. Invariante: un comando apunta a 1 store. |
| `entity` | string | Sí | Nombre de la entidad |
| `key` | string | Sí | Identificador único de la entidad (clave primaria) |
| `version` | u64 | Sí | Número de versión monotónico. El writer rechaza versiones menores o iguales a la almacenada en ese store. |
| `ts` | ISO 8601 | Sí | Timestamp del origen (no del writer). Last-write-wins. |
| `idempotency_key` | UUID v4 | Sí | Scope: `(store, key)`. El writer rechaza duplicados en el mismo store. |
| `ack_mode` | string enum | Sí | `accepted` (async) o `committed` (sync). |
| `payload` | object | Sí | Datos específicos de la entidad. Schema libre. |

## Envelope de evento

```json
{
  "event_id": "uuid-v4",
  "event_type": "pedido.creado",
  "store": "pedidos",
  "entity": "pedido",
  "key": "ped_abc123",
  "version": 7,
  "occurred_at": "2026-07-29T10:30:00.123Z",
  "outbox_seq": 1042,
  "payload": {}
}
```

| Campo | Tipo | Descripción |
|---|---|---|
| `event_id` | UUID v4 | Identificador único del evento. Usado para idempotencia de proyectores. |
| `event_type` | string | Tipo de evento en formato `<entity>.<accion>` (ej. `pedido.creado`, `usuario.actualizado`). |
| `store` | string | Store donde ocurrió el cambio. |
| `entity` | string | Entidad modificada. |
| `key` | string | Identificador de la entidad. |
| `version` | u64 | Versión resultante tras aplicar el comando. |
| `occurred_at` | ISO 8601 | Timestamp del commit (asignado por el writer). |
| `outbox_seq` | u64 | Secuencia del outbox en el store de origen. Para ordenamiento y debugging. |
| `payload` | object | Datos de la entidad tras el cambio. |

## Keyspace de FlashDB

```
c:<store>:<entity>:<key>     ← Cache-aside (lazy, se llena en read-miss)
p:<projection_name>:<key>    ← Read models (eager, se llena por proyección)
```

- Namespace `c:` gestionado por `ixmati-cache` (get/set/invalidate por TTL).
- Namespace `p:` gestionado por `ixmati-projector` (upsert por evento) y `ixmati-reconciler` (fan-in).
- La purga de `c:pedidos:*` no afecta `p:*` ni `c:usuarios:*`.
- El backend elige el camino de lectura: si pide por `projection`, la API busca en `p:`. Si pide por `store/entity/key`, busca en `c:`.

## Declaración de proyecciones (config TOML)

```toml
[[projections]]
name = "pedidos_con_usuario"
pattern = "R"                                          # R = referencia + lookup, M = materializado
source_stores = ["pedidos", "usuarios"]
target_key = "pedido_id"                                # campo del evento fuente que se usa como clave destino
ttl_seconds = 300

# Solo para patrón M:
# [[projections.copy_fields]]
# source_store = "usuarios"
# source_entity = "usuario"
# fields = ["nombre", "email"]
```

- `pattern = "R"`: el proyector guarda `{usr_id: 9}`. La consulta completa hace 2 lecturas a FlashDB y combina en Rust.
- `pattern = "M"`: el proyector copia campos. Requiere `copy_fields`. Validado contra regla de fan-out (DEC-0016: fan_out ≤ 100, ratio lectura/escritura ≥ 100:1).
- Sin proyecciones declaradas, el projector y el reconciler no hacen nada.

## Errores

- Usar `thiserror` para errores de dominio.
- Respuesta de error en la API:

```json
{
  "error": "VERSION_CONFLICT",
  "detail": "version 5 is not newer than stored version 7",
  "store": "pedidos",
  "idempotency_key": "abc-def"
}
```

- Códigos de error: `VERSION_CONFLICT`, `DUPLICATE`, `STORE_NOT_FOUND`, `ENTITY_NOT_FOUND`, `WRITE_REJECTED`, `QUEUE_FULL`, `PROJECTION_ERROR`, `INTERNAL`.
- Nunca exponer stack traces ni detalles internos de SQLite.

## Logging estructurado

- `tracing` con spans para trazabilidad extremo a extremo.
- Todo comando procesado loguea: `store`, `idempotency_key`, `entity`, `key`, `op`, `version`, `ack_mode`, `latency_ms`.
- Todo evento publicado loguea: `store`, `event_id`, `event_type`, `entity`, `key`, `version`, `outbox_seq`.
- Toda proyección actualizada loguea: `projection`, `event_id`, `source_store`, `latency_ms`.
- Niveles: `ERROR` (pérdida de datos, crash), `WARN` (rechazo, reintento, lag elevado), `INFO` (commit, evento publicado, proyección actualizada), `DEBUG` (detalle), `TRACE` (cache operations).

## Tests

- Tests unitarios: `#[cfg(test)] mod tests` en el mismo archivo.
- Tests de integración: crate Rust `tests/integration/` miembro del workspace (`publish = false`).
- Tests de smoke: pytest sobre uv en `tests/smoke/`, caja negra contra docker compose.
- Tests de crash: scripts en `helpers/shell/kill9_writer.sh` y `tests/smoke/test_crash_durability.py`.
- Cobertura: ratchet versionado en `.coverage-floor`, gate en `just test-cov-gate`. Ver DEC-0025.

## Tooling

### Contrato make vs just

- `make`: construye artefactos (compilar, codegen proto, Docker, dist). Ver `Makefile` y `helpers/make/*.mk`.
- `just`: task manager (test, lint, fmt, hooks, docs, CI). Ver `Justfile` y `helpers/just/*.just`.
- `just → make` permitido; `make → just` **prohibido** (guard automático en `lint_tool_boundary.py`). Ver DEC-0021.

### Naming de recetas

| Sistema | Naming | Ejemplo |
|---|---|---|
| Make targets | `kebab-case` | `build-release` |
| Just recipes | `kebab-case` | `test-unit` |
| Make modules | `snake_case.mk` | `common.mk` |
| Just modules | `snake_case.just` | `dev.just` |
| Shell scripts | `snake_case.sh` | `preflight.sh` |

### Python tooling

- `uv` gestiona Python ≥3.12. `pyproject.toml` en `helpers/python/`.
- Shebang: `#!/usr/bin/env uv run`. Nunca usar `python3` del sistema. Ver DEC-0023.

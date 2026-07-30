### Tabla de Referencia de Configuración

#### Global (global.toml)

| Campo | Tipo | Descripción |
|---|---|---|
| `data_dir` | string | Directorio de datos de SQLite |
| `log_level` | string | debug, info, warn, error |
| `mqtt.host` | string | Host del broker Mosquitto |
| `mqtt.port` | int | Puerto (default: 1883) |
| `cache.path` | string | Ruta del archivo FlashDB |

#### Store (stores.toml)

| Campo | Tipo | Obligatorio | Descripción |
|---|---|---|---|
| `name` | string | Sí | Identificador snake_case, inmutable |
| `path` | string | Sí | Ruta del archivo SQLite |
| `label` | string | No | Etiqueta descriptiva |
| `litestream.enabled` | bool | No | Activar replicación |
| `litestream.destinations` | []string | No | URLs S3 o filesystem |

#### Proyección (projections.toml)

| Campo | Tipo | Obligatorio | Descripción |
|---|---|---|---|
| `name` | string | Sí | Nombre único |
| `pattern` | "R" o "M" | Sí | R=lookup, M=materializado |
| `source_stores` | []string | Sí | Stores fuente |
| `target_key` | string | Sí | Clave destino en FlashDB |
| `ttl_seconds` | int | No | TTL (default: 300) |
| `copy_fields` | tabla | Solo M | Campos copiados |

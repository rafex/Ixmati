# COMMANDS.md

Comandos del proyecto para desarrollo, build, test y operaciones.

## Desarrollo local

```bash
# Compilar todo el workspace
cargo build

# Compilar en release
cargo build --release

# Ejecutar tests (unitarios + integración)
cargo test

# Tests con output
cargo test -- --nocapture

# Ejecutar un test específico
cargo test -p ixmati-writer -- test_batch_commit

# Linting
cargo clippy --all-targets --all-features -- -D warnings

# Formato
cargo fmt --all -- --check
cargo fmt --all

# Documentación
cargo doc --open
```

## Levantar servicios de desarrollo

```bash
# Iniciar Mosquitto + entorno de test
docker compose -f docker/docker-compose.dev.yml up -d

# Detener
docker compose -f docker/docker-compose.dev.yml down

# Ver logs de Mosquitto
docker compose -f docker/docker-compose.dev.yml logs -f mosquitto
```

## Stores

```bash
# Supervisor: lanzar con N stores (topología single-process)
cargo run -p ixmati-supervisor -- --config config/stores.toml

# Config de ejemplo (stores.toml):
# [[stores]]
# name = "pedidos"
# path = "/data/pedidos.db"
# label = "Pedidos (dominio)"
#
# [[stores]]
# name = "usuarios"
# path = "/data/usuarios.db"
# label = "Usuarios (dominio)"
```

## Operaciones de cache

```bash
# Purgar cache-aside de un store (no afecta proyecciones)
cargo run -p ixmati-cache -- purge --prefix "c:pedidos:*"

# Purgar un read model (requiere reproyección posterior)
cargo run -p ixmati-cache -- purge --prefix "p:pedidos_con_usuario:*"

# Ver estadísticas de FlashDB
cargo run -p ixmati-cache -- stats
```

## Reproyección (reconciler)

```bash
# Reproyectar todos los read models (fan-in sobre todos los stores)
cargo run -p ixmati-reconciler -- --config config/stores.toml --projections config/projections.toml

# Reproyectar una proyección específica
cargo run -p ixmati-reconciler -- --config config/stores.toml --projections config/projections.toml --projection pedidos_con_usuario

# Dry-run: validar sin escribir
cargo run -p ixmati-reconciler -- --config config/stores.toml --projections config/projections.toml --dry-run

# Opciones:
# --batch-size <N>       Stores procesados por batch (default: 10000)
# --projection <NAME>    Solo reproyectar una proyección
# --dry-run              Validar sin escribir en FlashDB
```

## Inspección de outbox

```bash
# Ver eventos pendientes de publicación en un store
sqlite3 /data/pedidos.db "SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL;"

# Ver últimos eventos publicados
sqlite3 /data/pedidos.db "SELECT event_id, event_type, entity, key, version, published_at FROM _outbox ORDER BY id DESC LIMIT 20;"

# Lag del publicador (diferencia entre último id y último published_at)
sqlite3 /data/pedidos.db "
  SELECT (SELECT MAX(id) FROM _outbox) - (SELECT MAX(id) FROM _outbox WHERE published_at IS NOT NULL) AS lag;
"
```

## Disaster recovery con Litestream

```bash
# Restaurar un store desde el último backup
litestream restore -o /data/pedidos_restored.db s3://ixmati-backups/pedidos

# Restaurar a un punto en el tiempo
litestream restore -o /data/pedidos_restored.db --timestamp 2026-07-29T10:30:00Z s3://ixmati-backups/pedidos

# Verificar integridad post-restauración
sqlite3 /data/pedidos_restored.db "PRAGMA integrity_check;"

# Ver generaciones de un store
litestream generations /data/pedidos.db

# Replicar un store específico (sidecar)
litestream replicate -config /etc/litestream/pedidos.yml
```

## Verificaciones de salud

```bash
# Health check de la API
curl http://localhost:8080/health

# Health check de un store específico
curl http://localhost:8080/health/pedidos

# Verificar integridad de un store
sqlite3 /data/pedidos.db "PRAGMA integrity_check; PRAGMA quick_check;"

# Tamaño del WAL de un store (bytes)
stat -f%z /data/pedidos.db-wal

# Verificar conexión Mosquitto
mosquitto_sub -t 'ixmati/health/#' -C 1 -W 2
```

## Métricas y debugging

```bash
# Lag de cola MQTT (comandos pendientes)
mosquitto_sub -t '$SYS/broker/messages/stored' -C 1

# Comandos procesados por store
sqlite3 /data/pedidos.db "SELECT COUNT(*) FROM _idempotency WHERE applied_at > datetime('now', '-1 hour');"

# Tamaño de cada store
sqlite3 /data/pedidos.db "SELECT page_count * page_size AS size_bytes FROM pragma_page_count(), pragma_page_size();"

# Eventos en outbox pendientes (todos los stores)
for db in /data/*.db; do echo "$db: $(sqlite3 "$db" 'SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL;')"; done
```

## ATTACH read-only (analítica ad-hoc)

```bash
# Abrir una sesión read-only con múltiples stores attachados
sqlite3 <<SQL
.mode json
ATTACH '/data/pedidos.db' AS pedidos;
ATTACH '/data/usuarios.db' AS usuarios;
SELECT p.id, p.total, u.nombre
FROM pedidos.pedido p
JOIN usuarios.usuario u ON p.usuario_id = u.id
WHERE p.created_at > datetime('now', '-7 days')
LIMIT 100;
SQL
```

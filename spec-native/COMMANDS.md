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

## Resync de cache

Reconstruir FlashDB desde SQLite (cache se considera siempre desechable):

```bash
cargo run -p ixmati-resync -- --sqlite /data/ixmati.db --cache /data/cache.db
```

Opciones:
```
--sqlite <PATH>       Ruta al archivo SQLite
--cache <PATH>        Ruta al archivo FlashDB (se sobrescribe)
--entity <ENTITY>     Solo reconstruir una entidad (opcional)
--batch-size <N>      Registros por batch (default: 10000)
```

## Disaster recovery con Litestream

```bash
# Restaurar desde el último backup
litestream restore -o /data/ixmati_restored.db s3://ixmati-backups/ixmati

# Restaurar a un punto en el tiempo
litestream restore -o /data/ixmati_restored.db --timestamp 2026-07-29T10:30:00Z s3://ixmati-backups/ixmati

# Verificar integridad post-restauración
sqlite3 /data/ixmati_restored.db "PRAGMA integrity_check;"
```

## Verificaciones de salud

```bash
# Health check de la API
curl http://localhost:8080/health

# Verificar integridad de SQLite
sqlite3 /data/ixmati.db "PRAGMA integrity_check; PRAGMA quick_check;"

# Verificar estado de replicación Litestream
litestream generations /data/ixmati.db

# Verificar conexión Mosquitto
mosquitto_sub -t 'ixmati/health/#' -C 1 -W 2

# Tamaño del WAL (bytes)
stat -f%z /data/ixmati.db-wal
```

## Métricas y debugging

```bash
# Lag de cola MQTT
mosquitto_sub -t '$SYS/broker/messages/stored' -C 1

# Tamaño de la base de datos
sqlite3 /data/ixmati.db "SELECT COUNT(*) FROM _idempotency; SELECT page_count * page_size AS size_bytes FROM pragma_page_count(), pragma_page_size();"
```

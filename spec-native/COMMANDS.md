# COMMANDS.md

Catálogo de comandos del proyecto. Todas las tareas de desarrollo se ejecutan con `just`. La compilación con `make`. Los comandos crudos se listan como referencia de "qué hace por debajo".

## Desarrollo diario

```bash
just doctor              # verificar entorno
just build               # compilar (→ make build)
just test                # todos los tests
just test-unit           # solo unitarios
just test-integration    # solo integración
just test-smoke          # solo smoke (pytest)
just test-cov            # cobertura + reporte HTML
just test-cov-gate       # cobertura con ratchet
just fmt                 # formatear código
just fmt-check           # verificar formato
just clippy              # linter
just quality             # fmt + clippy + boundary + validate
just boundary            # verificar make no llama a just
just validate-config     # validar stores.toml y projections.toml
just env-up              # levantar Mosquitto (Docker)
just env-down            # detener servicios
just logs <service>      # logs de un servicio
just watch               # recompilar al cambiar archivos
```

## Git hooks

```bash
just hooks-install       # instalar hooks (.githooks/)
just hooks-uninstall     # desinstalar
just hooks-pre-commit    # ejecutar pre-commit manualmente
just hooks-pre-push      # ejecutar pre-push manualmente
```

## Documentación

```bash
just docs-build          # compilar mdBook
just docs-serve          # mdBook con live reload (:3000)
just docs-check-links    # verificar links rotos
```

## Build y artefactos

```bash
make build               # compilar debug
make build-release       # compilar release
make proto               # generar código desde .proto
make docker              # construir imágenes Docker
make dist                # ensamblar dist/ con checksums
make clean               # limpiar target/ y dist/
```

## Stores

```bash
# supervisor multi-store (topología single-process)
cargo run -p ixmati-supervisor -- --config config/stores.toml
```

## Cache

```bash
# purgar cache-aside de un store
uv run python helpers/python/validate_config.py  # base para futuros comandos
```

## Reproyección

```bash
# reproyectar todos los read models
cargo run -p ixmati-reconciler

# reproyectar una proyección específica
cargo run -p ixmati-reconciler -- --projection pedidos_con_usuario

# dry-run
cargo run -p ixmati-reconciler -- --dry-run
```

## Inspección de outbox

```bash
sqlite3 /data/pedidos.db "SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL;"
sqlite3 /data/pedidos.db "SELECT event_id, event_type, entity, key, version, published_at FROM _outbox ORDER BY id DESC LIMIT 20;"
```

## Disaster recovery

```bash
litestream restore -o /data/pedidos_restored.db s3://ixmati-backups/pedidos
sqlite3 /data/pedidos_restored.db "PRAGMA integrity_check;"
```

## CI

```bash
just ci-pr               # verificaciones de PR
just ci-main              # verificaciones de main (con smoke + coverage)
```

## Health

```bash
curl http://localhost:8080/health
sqlite3 /data/pedidos.db "PRAGMA integrity_check;"
stat -f%z /data/pedidos.db-wal
```

### Qué hace cada just recipe por debajo

| Recipe | Comandos |
|---|---|
| `just test-unit` | `cargo test --lib --workspace` |
| `just test-integration` | `cargo test -p ixmati-integration` |
| `just test-smoke` | `uv run pytest tests/smoke/ -v` |
| `just test-cov` | `cargo llvm-cov --workspace --html` |
| `just fmt` | `cargo fmt --all` |
| `just clippy` | `cargo clippy --all-targets --all-features --workspace -- -D warnings` |
| `just audit` | `cargo deny check && cargo audit` |
| `just docs-build` | `mdbook build docs/` |
| `just docs-serve` | `mdbook serve --open docs/` |
| `just env-up` | `docker compose -f docker/docker-compose.dev.yml up -d` |

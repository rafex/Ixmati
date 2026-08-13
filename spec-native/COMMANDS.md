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
make proto               # validar todos los .proto con protoc
make docker              # construir imágenes Docker
make dist                # ensamblar dist/ con checksums
make clean               # limpiar target/ y dist/

# API binaria
cargo test -p ixmati-api  # conversiones Struct, cursor y contrato API
# REST JSON: 30000; REST/Protobuf: application/protobuf; gRPC: 30100
# readiness operativo: GET /ready (503 si un store o MQTT no está saludable)
# GRPC_PORT=0 deshabilita gRPC en despliegues legacy
```

## Instalador nativo (systemd)

Empaquetado del instalador (`dist/ixmati-<VERSION>-linux-amd64.tar.gz`):

```bash
make containers-builder      # imagen builder (compila los 6 binarios)
make containers-compile      # extrae binarios a target/release/
make dist                    # ensambla dist/ (incluye ixmati-cache-server)
make dist-checksums          # genera .sha256
make dist-validate           # verifica binarios/config/systemd en el tarball

# Operación offline de stores (requiere ventana y manifiesto quiesced=true)
ixmati-store-migrate plan --manifest benchmarks/migration.example.toml
ixmati-store-migrate verify --manifest migration.toml
ixmati-store-migrate execute --manifest migration.toml

# E2E offline migration + reconciler/cache en Debian efímero
helpers/shell/test_store_migration_e2e.sh

# Soak prolongado: un contenedor Debian nuevo por cada tasa
SOAK_RATES="150 200" DURATION=3600 DRAIN_SECONDS=300 \
  helpers/shell/run_soak_debian.sh

# Comparación JSON/REST-Protobuf/gRPC desde un generador dentro de Debian
PODMAN_CONNECTION=debian-server-wifi PODMAN_HOST_IP=127.0.0.1 \
  METRICS_URL=http://192.168.3.143:30000/metrics \
  RATES="40 100 150" DURATION=30 WARMUP=5 COOLDOWN=2 CONCURRENCY=200 \
  benchmarks/protocol_benchmark.sh
```

La carga se ejecuta dentro de un contenedor generador separado en el host
Debian (`--network host`) y llega al API por la IP local de ese host. El
operador sólo consulta el estado con Podman cada 15 minutos:

```bash
SOAK_RATES=150 DURATION=3600 CONCURRENCY=1000 \
  TEST_HOST=192.168.3.143 PODMAN_HOST_IP=127.0.0.1 \
  SNAPSHOT_INTERVAL=900 helpers/shell/run_soak_debian.sh
```

`TEST_HOST` es la dirección de control de la máquina Debian. El generador usa
`PODMAN_HOST_IP=127.0.0.1` porque corre en el namespace de red de esa máquina,
no en el equipo operador. El generador
se elimina al finalizar y el throttle temporal se restaura mediante `trap`.

Validación real en contenedor Debian con systemd como PID 1 (requiere Podman
`--privileged` + cgroups montados):

```bash
just installer-test          # → make installer-test: flujo completo
                              #   automatizado (instala, verifica round-trip
                              #   write/read, reinstala, desinstala --purge)
```

Operativa granular para depuración manual del contenedor de test:

```bash
make installer-test-image      # build imagen Debian + systemd
make installer-test-up         # levanta contenedor privilegiado
make installer-test-install    # copia dist/ y corre install.sh dentro
make installer-test-status     # systemctl is-active de los 5 servicios + /health
make installer-test-logs SVC=ixmati-api   # journalctl de un servicio
make installer-test-shell      # shell interactiva dentro del contenedor
make installer-test-uninstall  # corre install.sh --uninstall --purge
make installer-test-down       # detiene y elimina el contenedor
make installer-test-clean      # down + elimina la imagen de test
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

El helper reproducible selecciona la réplica local nativa cuando existe y
respeta `LITESTREAM_REPLICA_URL`, `LITESTREAM_S3_URL` o las variables del
instalador (`IXMATI_LITESTREAM_S3_BUCKET`/`IXMATI_LITESTREAM_S3_PREFIX`):

```bash
helpers/shell/litestream_restore.sh pedidos /tmp/pedidos-restored.db
```

Para un endpoint S3 no estándar se usa automáticamente
`/etc/ixmati/litestream-s3.yml` si existe; también puede indicarse otro con
`IXMATI_LITESTREAM_S3_CONFIG`. El helper termina con error si no encuentra una
réplica explícita o local, para no restaurar accidentalmente desde una URL de
ejemplo.

En la instalación nativa, Litestream `0.5.16` se instala en
`/usr/local/lib/ixmati/litestream`. La réplica local se gestiona con
`ixmati-litestream-file.service`; la réplica S3 es opcional y se habilita con
`IXMATI_LITESTREAM_S3_BUCKET` antes de instalar o actualizar:

```bash
systemctl status ixmati-litestream-file
systemctl status ixmati-litestream-s3
/usr/local/lib/ixmati/litestream restore \
  -o /tmp/pedidos-restored.db \
  file:///var/lib/ixmati/backups/pedidos.db
sqlite3 /tmp/pedidos-restored.db "PRAGMA integrity_check;"
```

Variables soportadas por el instalador: `IXMATI_LITESTREAM_S3_BUCKET`,
`IXMATI_LITESTREAM_S3_PREFIX`, `IXMATI_LITESTREAM_S3_REGION`,
`IXMATI_LITESTREAM_S3_ENDPOINT`, `AWS_ACCESS_KEY_ID`,
`AWS_SECRET_ACCESS_KEY` y `AWS_SESSION_TOKEN`. El restore local está validado;
el restore remoto, el segundo destino y las mediciones RPO/RTO siguen siendo
gates operativos separados.

Smoke test S3-compatible reproducible, sin tocar los stores del host:

```bash
helpers/shell/test_litestream_s3.sh
```

El arnés usa MinIO y Debian fijados por digest, verifica que existan objetos
LTX y restaura una base con `integrity_check`. No mide RPO/RTO de producción.

## CI

```bash
just ci-pr               # verificaciones de PR
just ci-main              # verificaciones de main (con smoke + coverage)

## Capacidad prolongada

```bash
# JMeter: una hora a 150/s; repetir con rate=200
just benchmark-soak 150 3600 200

# Harness completo: snapshots de API/writer/MQTT y drenado
SOAK_RATES="150 200" DURATION=3600 benchmarks/soak_capacity.sh
```
```

## Health

```bash
curl http://localhost:8080/health
sqlite3 /data/pedidos.db "PRAGMA integrity_check;"
stat -f%z /data/pedidos.db-wal
```

## Autenticación

```bash
# Arrancar la API con API keys (separadas por coma)
IXMATI_API_KEYS="ix-key-1,ix-key-2" cargo run -p ixmati-api

# Llamada autenticada a POST /write
curl -X POST http://localhost:8080/write \
  -H "Authorization: ApiKey ix-key-1" \
  -H "Content-Type: application/json" \
  -d '{"op":"upsert","store":"pedidos","entity":"pedido","key":"p1","version":1,"ts":"2026-01-01T00:00:00Z","idempotency_key":"550e8400-e29b-41d4-a716-446655440000","ack_mode":"accepted","payload":{}}'

# Llamada con Bearer token
curl -X POST http://localhost:8080/write \
  -H "Authorization: Bearer ix-session-token" \
  -H "Content-Type: application/json" \
  -d '{...}'

# Sin auth → rechazo 401
curl -X POST http://localhost:8080/write \
  -H "Content-Type: application/json" \
  -d '{...}'

# Rutas públicas (no requieren auth)
curl http://localhost:8080/health
curl http://localhost:8080/writes/pedidos/550e8400-e29b-41d4-a716-446655440000
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
| `just installer-test` | `helpers/shell/test_installer_debian.sh` (vía `make installer-test`) |

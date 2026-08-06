# TODO.md

Tablero de tareas activo. Persiste entre sesiones.

## Active — Write Engine (SPEC-WRITE-0001)

- [x] `TASK-WRITE-0001` — Spike: viabilidad de FlashDB vía FFI en Rust
- [x] `TASK-WRITE-0003` — Definir contrato de envelope, topics MQTT y .proto
- [x] `TASK-WRITE-0004` — Definir contrato de API REST (OpenAPI)
- [x] `TASK-WRITE-0005` — Implementar ixmati-core
- [x] `TASK-WRITE-0006` — Implementar ixmati-writer
- [x] `TASK-WRITE-0007` — Tests de crash del writer
- [x] `TASK-WRITE-0008` — Implementar ixmati-api
- [x] `TASK-WRITE-0009` — Implementar modo async y sync
- [x] `TASK-WRITE-0010` — Endpoint GET /writes/{store}/{idempotency_key}
- [x] `TASK-WRITE-0011` — Implementar ixmati-cache
- [x] `TASK-WRITE-0012` — Invalidación/repoblación de cache-aside
- [x] `TASK-WRITE-0014` — Configurar Litestream por store
- [x] `TASK-WRITE-0015` — Health checks integrados
- [x] `TASK-WRITE-0016` — Documentar runbook de producción
- [x] `TASK-WRITE-0017` — Store registry + config multi-store
- [x] `TASK-WRITE-0018` — Tabla _outbox + publicador transaccional
- [x] `TASK-WRITE-0019` — EventEnvelope + bus de eventos
- [x] `TASK-WRITE-0020` — ixmati-projector
- [x] `TASK-WRITE-0021` — Declaración de proyecciones en config
- [x] `TASK-WRITE-0022` — ixmati-reconciler
- [x] `TASK-WRITE-0023` — ATTACH read-only
- [x] `TASK-WRITE-0024` — ixmati-supervisor + K8s manifests
- [x] `TASK-WRITE-0025` — Tests de consistencia eventual

## Active — Containers (SPEC-CONTAINERS-0001)

- [x] `TASK-CONT-0001` — .containerignore
- [x] `TASK-CONT-0002` — Builder compartido cargo-chef
- [x] `TASK-CONT-0003` — Containerfiles de servicios
- [x] `TASK-CONT-0004` — Imagen Mosquitto
- [x] `TASK-CONT-0005` — Imagen Litestream
- [x] `TASK-CONT-0006` — Compose dev + test
- [x] `TASK-CONT-0007` — Compose single-store + multi-store
- [x] `TASK-CONT-0008` — Quadlet units
- [x] `TASK-CONT-0009` — Helpers y make/just
- [x] `TASK-CONT-0010` — Migrar referencias docker → podman
- [x] `TASK-CONT-0011` — CI con podman
- [x] `TASK-CONT-0012` — Validación end-to-end

## Active — Smoke Tests (SPEC-SMOKE-0001)

- [x] `TASK-SMOKE-0001` — Corregir bugs de infraestructura (build, network, healthcheck)
- [x] `TASK-SMOKE-0002` — Ejecutar 12 tests E2E restantes y verificar resultados
- [x] `TASK-SMOKE-0003` — Extender podman_tunnel.sh con port forwards automáticos
- [x] `TASK-SMOKE-0004` — Registrar pytest.mark.smoke en pyproject.toml
- [x] `TASK-SMOKE-0005` — Registrar decisiones (DEC-0025, DEC-0026) en DECISIONS.md

## Active — Tooling (SPEC-TOOL-0001)

- [x] `TASK-TOOL-0001` — helpers/python con uv
- [x] `TASK-TOOL-0002` — helpers/shell/lib.sh + preflight.sh
- [x] `TASK-TOOL-0003` — lint_tool_boundary.py
- [x] `TASK-TOOL-0004` — Makefile thin + helpers/make/*.mk
- [x] `TASK-TOOL-0005` — Justfile thin + helpers/just/*.just
- [x] `TASK-TOOL-0006` — .githooks/ + just hooks-install
- [x] `TASK-TOOL-0007` — Workspace Cargo con 7 crates
- [x] `TASK-TOOL-0008` — tests/integration como crate miembro
- [x] `TASK-TOOL-0009` — tests/smoke pytest + fixtures
- [x] `TASK-TOOL-0010` — Ratchet de cobertura
- [x] `TASK-TOOL-0011` — Validadores
- [x] `TASK-TOOL-0012` — docs/ con mdBook
- [x] `TASK-TOOL-0013` — Pipelines CI.md + CD.md
- [x] `TASK-TOOL-0014` — GitHub Actions CI

## Active — Cache Backend (SPEC-CACHE-0001)

- [x] `TASK-CACHE-0001` — FlashDB backend
- [x] `TASK-CACHE-0002` — SQLite backend (WAL multi-proceso)
- [x] `TASK-CACHE-0003` — Redb backend
- [x] `TASK-CACHE-0004` — ReadOnlyCache wrapper
- [x] `TASK-CACHE-0005` — CacheProxy MQTT + CacheResponder
- [x] `TASK-CACHE-0006` — CacheClient/Server socket IPC
- [x] `TASK-CACHE-0007` — Benchmark comparativa (direct/socket/mqtt, 1-1000 conc)
- [x] `TASK-CACHE-0008` — DEC-0036 Redb+Socket como default de producción

## Active — Projector Validation (SPEC-PROJECTOR-0001)

- [x] `TASK-PRJ-0001` — Fase 0: EventPublisher emite EventEnvelope completo
- [x] `TASK-PRJ-0002` — Fase 1: Protocolo socket extendido (GET/SET/DEL/DEL_PREFIX/FLUSH) en ixmati-cache
- [x] `TASK-PRJ-0003` — Fase 1: Binario ixmati-cache-server + Containerfile
- [x] `TASK-PRJ-0004` — Fase 1: Keyspace unificado p: en pattern_r/pattern_m
- [x] `TASK-PRJ-0005` — Fase 1: CacheSync vía socket client (writer ya no abre Redb directo)
- [x] `TASK-PRJ-0006` — Fase 4: API ?projection=&key= implementado
- [x] `TASK-PRJ-0007` — Fase 2: Projector real (MQTT consumer + dedup event_id + socket SET)
- [x] `TASK-PRJ-0008` — Fase 3: Reconciler real (fan-in stores + socket SET)
- [x] `TASK-PRJ-0009` — Fase 5: Multi-store compose con cache-server
- [x] `TASK-PRJ-0010` — Fase 6: All-in-one supervisord.conf con cache-server
- [x] `TASK-PRJ-0011` — Fase 7: Validación e2e e-commerce (CA-11, CA-12, CA-13)
- [x] `TASK-PRJ-0012` — Fase 7: DEC de cierre + SESSION/TODO actualizado

- [x] `TASK-AUTH-0001` — Definir modelo de sesión
- [x] `TASK-AUTH-0002` — Implementar middleware de autorización
- [x] `TASK-AUTH-0003` — Documentar setup operativo

## Pendiente (post-v0.1.0)

- [x] Fijar versiones de dependencias wildcard
- [x] Crear K8s manifests (Deployment, PVC, Litestream sidecar)
- [x] Agregar métricas Prometheus (endpoint + counters/histograms/gauges)
- [x] `cargo audit` en CI pipeline
- [x] Implementar FlashDB backend en `ixmati-cache` (compila con --features flashdb)
- [x] Backpressure (rechazar comandos con cola llena — sliding window)
- [x] Alertas operativas (Prometheus AlertManager rules)
- [x] Benchmarks de throughput multi-store
- [x] Clippy warnings corregidos
- [x] Writer standby con failover automático
- [x] CDC para suscriptores externos vía MQTT
- [x] Smoke tests implementados (5 tests reales contra podman compose)
- [x] Compose smoke.yaml (stack completo para tests)
- [x] Fix env vars en single-store.yaml (MQTT_BROKER)
- [x] API lee SQLITE_PATH como env var fallback
- [x] Receta `just smoke` (levantar + test + teardown)
- [ ] Sharding interno de un store
- [ ] Dashboard web de operación
- [ ] Migración de stores (renombrar, merge, split)

## Cancelled / Replaced

- [x] `TASK-WRITE-0002` — Spike comparativa Opción A vs B (cerrada por diseño)
- [x] `TASK-WRITE-0013` — ixmati-resync (reemplazada por reconciler fan-in)

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

## Active — Native Installer Hardening (SPEC-INSTALL-0001)

- [x] `TASK-INST-0001` — Unificar env vars de projector/reconciler con el resto
      del sistema (`MQTT_BROKER`/`CACHE_SOCKET_PATH` en vez de
      `IXMATI_MQTT_BROKER`/`IXMATI_CACHE_SOCKET`)
- [x] `TASK-INST-0002` — Unidad systemd `ixmati-cache-server.service` +
      dependencias `Requires=`/`After=` en api/writer/projector
- [x] `TASK-INST-0003` — `installer.py`: empaquetar y arrancar cache-server y
      projector, no sobrescribir config existente, `verify_health()`
- [x] `TASK-INST-0004` — `installer.py --uninstall` / `--uninstall --purge`
- [x] `TASK-INST-0005` — Incluir `ixmati-cache-server` en `make dist` /
      `make dist-validate`
- [x] `TASK-INST-0006` — Validación real del instalador en contenedor Debian
      (`containers/installer-test/` + `helpers/shell/test_installer_debian.sh`
      + `just installer-test`)

## Active — Validation & Load Testing (SPEC-VAL-0001)

- [x] `TASK-VAL-0001` — Extender Prometheus metrics:
      `ixmati_process_memory_rss_bytes`, `ixmati_process_cpu_user_seconds_total`,
      `ixmati_write_batch_duration_seconds`
- [x] `TASK-VAL-0002` — Crear `helpers/shell/test_stack_validation.sh`:
      write/read round-trip, idempotencia, load testing (100 ops/s).
      NOTA: no valida proyecciones — la config de instalación default es
      single-store y `projections.toml` (pattern R/M) requiere stores
      `pedidos`/`usuarios` que no existen en esa config
- [x] `TASK-VAL-0003` — Crear load test harness (integrado en test_stack_validation.sh):
      constant throughput, recolecta métricas y percentiles p50/p99 en paralelo
- [x] `TASK-VAL-0004` — Ejecutar validación end-to-end en Debian container:
      instala stack, valida, corre carga, reporta métricas.
      Resultado: 5/5 servicios estables, write/read/idempotencia OK,
      8545 writes secuenciales en 30s (285 ops/s, p50=2ms, p99=3ms, 0 errores),
      RSS: writer 8.3MB, projector 7.7MB, api 6.1MB, cache-server 3.9MB.
      GAP encontrado: `ixmati_write_requests_total`, `write_latency_seconds`,
      `write_errors_total`, `outbox_size`, `projection_lag_events` y las 3
      métricas nuevas de VAL-0001 están registradas pero nunca se
      incrementan/observan en el código — solo `cache_hits/misses_total` y
      `queue_depth` tienen call sites reales en `rest.rs`. Además hay un bug
      de doble prefijo: `Registry::new_custom(Some("ixmati"))` +
      nombres que ya empiezan con `ixmati_` produce
      `ixmati_ixmati_*` en `/metrics`.
- [x] `TASK-VAL-0005` — Migrar instrumentación de Prometheus directo a
      OpenTelemetry (vendor-neutral) en `ixmati-api`: `crates/ixmati-api/src/metrics.rs`
      reescrito con `opentelemetry`/`opentelemetry_sdk` 0.32 + exporter
      `opentelemetry-prometheus` 0.32 (mantiene `/metrics` en formato
      Prometheus, sin romper scrapers existentes). De paso corrige el bug
      de doble prefijo (namespace se configura una sola vez via
      `.with_namespace("ixmati")` en el exporter, no en el nombre de cada
      métrica). Call sites en `rest.rs` (QUEUE_DEPTH, CACHE_HITS,
      CACHE_MISSES) migrados a la API de atributos de OTel
      (`KeyValue`/`.add()`/`.record()`). Validado con test de regresión
      (`metrics::tests::encode_metrics_has_single_ixmati_prefix...`) y con
      tráfico real en el contenedor Debian: `/metrics` devuelve
      `ixmati_cache_hits_total{namespace="cache",store="default",...}` sin
      duplicación. `cargo test --workspace --lib` sigue en verde (100+ tests).
      Pendiente (no en este alcance): instrumentar WRITE_REQUESTS/WRITE_LATENCY/
      WRITE_ERRORS/OUTBOX_SIZE/PROJECTION_LAG/PROCESS_* en sus call sites reales.
- [x] `TASK-VAL-0006` — Instrumentar métricas pendientes de VAL-0005 + load test
      concurrente real:
      1. `write_handler` (`rest.rs`) ahora incrementa `WRITE_REQUESTS`,
         `WRITE_ERRORS` (con `error_type`: queue_full/serialize/mqtt_unavailable/
         mqtt_publish) y observa `WRITE_LATENCY` en todo camino de salida.
      2. Nuevo módulo `crates/ixmati-api/src/self_monitor.rs`: tarea de fondo
         cada 5s que lee `/proc/self/status` (VmRSS → `PROCESS_MEMORY_RSS`) y
         `/proc/self/stat` (utime delta → `PROCESS_CPU_USER`, asume
         `CLK_TCK=100`, estándar en Linux/Debian). Si `SQLITE_PATH` está
         configurado, también consulta `SELECT store, COUNT(*) FROM _outbox
         WHERE published_at IS NULL GROUP BY store` → `OUTBOX_SIZE`.
      3. `helpers/shell/test_stack_validation.sh`: `load_test()` reescrito de
         loop secuencial de curl a N workers concurrentes en subshells de
         bash (default `LOAD_CONCURRENCY=20`), cada uno con su propio archivo
         de resultados, agregados al final.
      4. `PROJECTION_LAG` y `WRITE_BATCH_DURATION` siguen sin instrumentar:
         requieren datos que solo existen en `ixmati-projector`/`ixmati-writer`,
         procesos que no exponen `/metrics` — instrumentarlos de verdad
         implicaría agregar un endpoint de métricas a esos binarios, fuera
         de alcance de esta tarea.
      Validado con tráfico real en Debian (`make installer-test` +
      reinstalación con binarios recompilados): con `LOAD_CONCURRENCY=20`
      durante 30s se lograron 13460 writes (448.7 ops/s reales, vs 285 ops/s
      del loop secuencial anterior), `ixmati_write_requests_total` coincidió
      exactamente con `ops_done` del script (13460), `PROCESS_MEMORY_RSS`
      reportó 7.4MB reales, `PROCESS_CPU_USER` 0.33s acumulados. `OUTBOX_SIZE`
      se verificó por separado con `SQLITE_PATH` forzado vía override de
      systemd (el instalador nativo no lo setea por defecto — el API en modo
      `CACHE_READ_MODE=socket` no necesita SQLite directo salvo para esta
      métrica) y reveló un backlog real de 4762 eventos sin publicar tras la
      carga — el writer no vació la cola al ritmo de escritura, dato genuino
      que amerita seguimiento (no corregido, fuera de alcance de esta tarea).
      HALLAZGO IMPORTANTE: `WRITE_LATENCY` medida en servidor (que solo cubre
      publish a un canal MQTT en memoria, no un round-trip real) dio un
      promedio de ~2.4 microsegundos, mientras el script de carga midió
      p50=37ms/p99=66ms del lado cliente (curl vía HTTP). La brecha de 4
      órdenes de magnitud es real y se debe a que el script gasta un proceso
      `curl` nuevo por request sin keep-alive, más contención de CPU entre 20
      workers de bash+curl en paralelo — no es tiempo de procesamiento del
      servidor. Los números de "ops/s" y latencia del load test reflejan el
      techo del harness de bash, no necesariamente el techo real de
      `ixmati-api`. Para medir capacidad real del servidor haría falta un
      cliente de carga sin ese overhead (ej. `hey`, `vegeta`, o un cliente
      async en Rust/Python) — queda como mejora pendiente, no bloqueante.

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

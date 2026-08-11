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
- [x] `TASK-VAL-0007` — Análisis de viabilidad para alto volumen/multi-tenant:
      investigada la causa raíz del backlog de outbox (TASK-VAL-0006) y los
      límites estructurales de escalabilidad multi-tenant. Veredicto y
      backlog priorizado registrados en DEC-0043. Resumen: el motor de
      escritura (outbox transaccional, idempotencia, single-writer-por-store)
      es sólido y no requiere rediseño; el backlog de outbox es un bug barato
      en `event_publisher.rs` (intervalo/límite hardcodeados + conexión SQLite
      por fila), no un límite arquitectónico; el cache-server centralizado
      (`std::sync::Mutex<Database>` único para todos los stores/tenants) SÍ
      es un techo real para multi-tenant de alto volumen, ya documentado sin
      profundizar en DEC-0036/DEC-0037. No viable *hoy* para ese caso de uso
      sin ejecutar el backlog de abajo.

## Backlog priorizado — viabilidad alto volumen/multi-tenant (DEC-0043)

- [x] `TASK-VAL-0008` (P0, bajo esfuerzo/alto impacto) — Reescribir
      `crates/ixmati-writer/src/event_publisher.rs`: `publish_unpublished`
      ahora fetch-ea el batch con una sola conexión, publica todos los
      eventos concurrentemente vía `tokio::task::JoinSet` (antes: secuencial
      con `.await` por evento), y marca los exitosos con un único
      `Outbox::mark_published_batch` (`UPDATE ... WHERE id IN (...)`, nuevo
      en `outbox.rs`) en vez de abrir una conexión SQLite nueva por fila.
      `PUBLISH_INTERVAL_MS` (default 200, antes 1000 hardcodeado) y
      `PUBLISH_BATCH_LIMIT` (default 500, antes 100 hardcodeado) ahora son
      env vars configurables en `main.rs`. 2 tests nuevos en `outbox.rs`
      (`mark_published_batch_marks_all_given_ids`,
      `mark_published_batch_empty_is_noop`). `cargo test --workspace --lib`
      sigue en verde (100+ tests, incluye los 2 nuevos).
      **Validado con el mismo load test en Debian real, antes/después**:
      antes del fix, 4762/13460 eventos sin publicar tras la carga (~35%,
      confirmado por consulta SQL directa además del gauge). Después del
      fix, con carga equivalente (12753 writes, 425 ops/s reales,
      concurrencia 20, 30s), **0/12755 eventos sin publicar** (confirmado
      por `SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL` vía
      python3/sqlite3 dentro del contenedor, no solo la métrica). Los 5
      servicios systemd siguieron activos durante y después de la carga.
- [x] `TASK-VAL-0009` (P1, bloqueante para producción segura) — Backpressure
      real: nuevo módulo `crates/ixmati-api/src/backpressure.rs`
      (`OutboxBacklog`) mantiene el último `OUTBOX_SIZE` conocido por store
      (poblado por `self_monitor.rs` en cada poll de 5s) y `write_handler`
      rechaza con `429` + `Error::OutboxBacklog` (nueva variante en
      `ixmati-core`) cuando el backlog de un store supera
      `OUTBOX_BACKPRESSURE_THRESHOLD` (default 5000, `<=0` deshabilita). De
      paso se corrigió un bug real que esto expuso: `collect_outbox_metrics`
      solo agrupaba stores CON backlog (`GROUP BY` sin filas para stores ya
      drenados), así que tanto la métrica como el backlog cacheado se
      quedaban con el último valor != 0 para siempre una vez que el writer
      alcanzaba a drenar — `reset_missing()` ahora pone en 0 explícitamente
      los stores que desaparecen del resultado. 6 tests nuevos
      (`backpressure.rs`: bajo umbral, sobre umbral, store desconocido,
      umbral 0 deshabilita, reset de stores drenados, reset es idempotente).
      **Validado en Debian real de punta a punta**: con el writer detenido
      y 50 filas sintéticas insertadas directo en `_outbox` (umbral=20), un
      `POST /write` fue rechazado con `429 {"OutboxBacklog":{"depth":50,
      "threshold":20}}` y el log estructurado correspondiente; al reiniciar
      el writer y esperar el drenado, `OUTBOX_SIZE` volvió a `0` explícito
      (ya no desaparece la serie) y el siguiente write fue aceptado (200)
      automáticamente, sin reiniciar el API. `WRITE_ERRORS{error_type=
      "outbox_backlog"}` quedó contabilizado correctamente.
- [x] `TASK-VAL-0010` (P2, decisión de diseño) — Investigado el
      `Mutex<Database>` de `RedbCacheBackend`
      (`crates/ixmati-cache/src/redb_backend.rs`): resultó ser un lock
      externo redundante, no una necesidad real. El propio código fuente de
      redb 4.1.0 documenta que `Database::begin_write(&self)` ya serializa
      internamente ("only a single write may be in progress at a time...
      this function will block until it completes") y que
      `Database::begin_read(&self)` soporta lecturas concurrentes reales sin
      lock externo — `Database` es `Send + Sync` de fábrica. El `Mutex`
      forzaba a que TODAS las lecturas de TODOS los stores/tenants hicieran
      cola entre sí, cuando redb ya las permite en paralelo. Se eliminó el
      `Mutex`, `RedbCacheBackend` ahora sostiene `Database` directamente.
      Además, `crates/ixmati-cache/src/cache_server.rs` despacha ahora cada
      operación (`GET`/`SET`/`DEL`/`DEL_PREFIX`/`FLUSH`) a
      `tokio::task::spawn_blocking`, así una transacción larga no bloquea
      los hilos del executor async que atienden el resto de las conexiones
      del socket. `cargo check` confirmó en el momento que `Database` sigue
      cumpliendo `Send + Sync` sin el `Mutex` (si no lo fuera, el trait
      bound `CacheBackend: Send + Sync` no habría compilado). Validado con
      el mismo load test en Debian real tras el cambio: 13382 writes, 0
      errores, comportamiento idéntico al anterior. NOTA: esto resuelve la
      serialización *interna* del proceso cache-server; no es sharding por
      store ni multi-proceso — sigue siendo un solo proceso/archivo Redb
      para todos los tenants (mitigado, no eliminado, como punto único de
      fallo — ver DEC-0037). Sharding real por store (múltiples archivos
      Redb o particionamiento) queda fuera de este alcance si se necesita
      aislamiento de fallos entre tenants, no solo throughput.
- [x] `TASK-VAL-0011` (P3) — Load test real sin overhead de proceso-por-request:
      `wrk` (disponible en apt de Debian trixie) + script Lua nuevo
      (`helpers/wrk/write.lua`) contra `POST /write`, corrida sostenida de 60s
      y de 3 min, con `MAX_WRITES_PER_WINDOW` elevado temporalmente (el
      default de 1000/s/store rechazaba casi todo con 429 antes de llegar a
      medir nada — ver DEC-0046 para el detalle de por qué eso también es un
      hallazgo real, no solo un obstáculo del test).
      **Resultado real**: 44,281-45,531 req/s sostenidos (3 min y 60s
      respectivamente), 0 errores HTTP, p50=0.35-0.89ms, p99=2.57-34.75ms,
      RSS estable ~11MB, los 5 servicios activos durante y después.
      Comparado con los 425-448 ops/s medidos con el harness de bash+curl
      (DEC-0042): confirma que ese número anterior efectivamente medía el
      harness, no el servidor — el techo real de aceptación de la API es
      ~100x mayor.
      **CAVEAT CRÍTICO, no resuelto**: `wrk` corrió DENTRO del mismo
      contenedor que los 5 servicios de Ixmati, sobre una VM de podman con
      solo 2 vCPUs asignadas — compitiendo por los mismos cores que
      `ixmati-api`/`ixmati-writer`. Se observó que bajo esta carga el
      `ixmati-writer` solo lograba comprometer ~100 batches/s a SQLite (vía
      logs de `journalctl`), muy por debajo de los ~425/s vistos en el test
      más liviano de DEC-0044 — probable contención de CPU con el propio
      generador de carga, no necesariamente un techo real del writer. No se
      puede separar limpiamente "capacidad del servidor" de "contención con
      el harness" en este entorno. Un número de capacidad limpio requeriría
      correr `wrk` en una máquina/contenedor separado del target.
- [x] `TASK-VAL-0013` (nuevo, descubierto durante P3) — El rate-limiter
      default (`MAX_WRITES_PER_WINDOW=1000` escrituras/s por store,
      `throttle.rs`) rechazaba con `429 QueueFull` cualquier carga
      sostenida por encima de ese umbral, independientemente de la
      capacidad real del servidor. Resuelto en `TASK-VAL-0020`/DEC-0054:
      default recalibrado a 40/s (capacidad real medida) y documentado
      explícitamente en `systemd/ixmati-api.service` con las líneas
      `Environment=` comentadas listas para ajustar — ya no es una decisión
      de producto implícita y sin visibilidad.
- [x] `TASK-VAL-0014` (limpieza del load test) — `wrk` instalado en el host
      macOS (10 cores) vía Homebrew, contenedor con puerto 30000 publicado
      (`podman run -p 30000:30000`), carga sostenida de 3 min desde fuera de
      la VM de podman. **Resultado limpio**: 17,960 req/s, 0 errores HTTP,
      p50=2.36ms — más bajo que los 44k del intento anterior (esperable: ya
      no hay 2 procesos compitiendo por los mismos 2 vCPUs, así que el
      contenedor de 2 vCPUs es ahora el límite real, no el generador de
      carga muriendo de hambre de CPU).
      **HALLAZGO CRÍTICO, no buscado, mucho más importante que el número de
      req/s**: de las 3,232,239 escrituras `ACCEPTED`, solo 27,400 (0.85%)
      llegaron a comprometerse de verdad en `_outbox` — `ixmati-writer`
      murió por **OOM-kill dos veces** durante la corrida (confirmado por
      `systemctl status`), precedido por errores `"database is locked"` y
      `"Broken pipe"` en su cliente MQTT. Se confirmó con 3 evidencias
      independientes (ver DEC-0048 para el detalle completo) que el
      `duplicates=0` en los logs del writer descarta que fuera una colisión
      de `idempotency_key` del test — es pérdida real: `rumqttc::
      AsyncClient::publish()` solo confirma encolado local (`client.rs:87`,
      `request_tx.send_async(...).await?`), no entrega real al broker; si el
      `eventloop` del cliente falla en medio (como pasó, por presión de
      memoria), esos mensajes se pierden sin que el código que llamó a
      `publish()` se entere — ya había devuelto `Ok(())`. Mosquitto mismo no
      descartó nada (`$SYS/broker/load/publish/dropped/*min = 0.00` en todas
      las ventanas). Hallazgo secundario: `ack_mode: "committed"` no espera
      ni verifica nada distinto de `"accepted"` — el nombre es engañoso.
      **Esto revisa a la baja el veredicto de DEC-0043**: el motor es sólido
      en el camino feliz probado hasta ahora (≤450 ops/s reales en
      DEC-0044/0045), pero bajo la carga que la propia capa HTTP puede
      aceptar sin rechazar, puede reconocer como "ACCEPTED" escrituras que
      nunca se persisten. Ver `TASK-VAL-0015..0017` para el seguimiento.
- [x] `TASK-VAL-0017` (P1 de DEC-0049, hecho primero — barato) —
      `crates/ixmati-writer/src/db.rs` nuevo (`open_with_pragmas`), aplica
      `PRAGMA busy_timeout=5000` (antes ausente) en los 4 sitios donde el
      writer abría conexiones SQLite (`main.rs` x2, `event_publisher.rs`
      x2). 1 test nuevo. **Validado en Debian**: 0 líneas "database is
      locked" en 3 min de carga sostenida (antes: repetidas).
- [x] `TASK-VAL-0016` (P2 de DEC-0049) — `crates/ixmati-writer/src/
      consumer.rs`: `mpsc::unbounded_channel()` (causa del crecimiento de
      memoria sin límite) → `mpsc::channel(capacity)` (`CONSUMER_CHANNEL_
      CAPACITY`, default 5000) + `mqtt_options.set_manual_acks(true)` —
      solo se ackea al broker lo que efectivamente entra al canal acotado;
      si está lleno, el mensaje queda sin ackear y Mosquitto lo
      redistribuye (backpressure trasladada al broker, que ya tiene su
      propio límite conocido, en vez de RAM sin límite del writer).
      Mensajes que fallan deserialización se ackean igual (evita loop
      infinito de redelivery de mensajes envenenados). Métrica nueva
      `CONSUMER_QUEUE_DEPTH`. 2 tests nuevos (mecanismo real de
      `try_send`, sin necesitar broker). **Validado en Debian repitiendo
      la MISMA carga de 3 min que causó el OOM en TASK-VAL-0014**: 0
      reinicios del writer (antes: 2 OOM-kills), memoria estable 15.3MB
      pico 15.9MB (antes: crecimiento sin límite), 5,743,227 requests,
      31,903 req/s, 0 errores HTTP, 0 timeouts (antes: 54), p99=4.14ms
      (antes: 720.66ms).
- [x] `TASK-VAL-0015` (P3 de DEC-0049) — `write_handler` (`rest.rs`) hace
      polling real contra `_idempotency` (reutilizando `StatusQuery::
      query`) cuando `ack_mode: "committed"`, antes de responder. Confirma
      `200 APPLIED` con datos reales si comete dentro de
      `WRITE_COMMITTED_TIMEOUT_MS` (default 2000ms), o `202 PENDING`
      (nunca `"ACCEPTED"` falso) si vence el timeout. Sin `SQLITE_PATH`
      configurado, rechaza con `400` explícito en vez de degradar
      silenciosamente a `"accepted"`. Lógica extraída a `wait_for_commit()`
      testeable sin MQTT — 3 tests nuevos (aplicado inmediato, timeout,
      commit tardío dentro del deadline). **Validado en Debian bajo la
      misma sobrecarga**: con backlog activo, una escritura `committed`
      respondió honestamente `202 PENDING` en vez de un falso `200
      ACCEPTED`.
      **Hallazgo adicional de esta validación**: de 5.7M escrituras
      aceptadas, solo ~5400 se comprometieron en la ventana de 3 min — pero
      ya NO es pérdida silenciosa: `$SYS/broker/messages/stored` mostró
      100,093 en cola (el límite propio de Mosquitto,
      `max_queued_messages`) y `$SYS/broker/publish/messages/dropped`
      mostró ~5.6M descartados **de forma visible y contable**, no por un
      crash invisible. El techo real de comandos/s comprometidos a SQLite
      en este contenedor de 2 vCPUs sigue siendo bajo bajo esta carga
      extrema, pero ahora falla de forma segura, acotada y observable.
      DEC-0049 registrada, cierra el seguimiento de DEC-0048.
- [x] `TASK-VAL-0012` (P4, parcial — ver desglose) — De los 3 ítems de este
      punto, se hizo el primero y se dejaron los otros 2 explícitamente como
      roadmap, no como "hecho":
      1. **Instrumentado**: `WRITE_BATCH_DURATION` (`ixmati-writer`) y
         `PROJECTION_LAG` (`ixmati-projector`) — cada crate tiene ahora su
         propio `metrics.rs` (mismo patrón OTel+Prometheus que `ixmati-api`)
         y un endpoint `/metrics` HTTP **opt-in vía `METRICS_PORT`** (sin esa
         env var no se abre ningún puerto — a propósito, porque
         `ixmati-writer` corre como unidad systemd template
         `ixmati-writer@<store>`, una instancia por store, y un puerto fijo
         por defecto colisionaría entre instancias del mismo host).
         `WRITE_BATCH_DURATION` mide el tiempo real de
         `WriteEngine::process_batch` (SQLite). `PROJECTION_LAG` mide
         `ahora - occurred_at` del evento procesado (lag de tiempo, no de
         cantidad de eventos — el projector es un consumidor de stream MQTT
         puro, sin visibilidad de un "total publicado" independiente contra
         el cual contar backlog en eventos). 3 tests nuevos entre ambos
         crates. Validado en Debian real con `METRICS_PORT` configurado por
         override: `write_batch_duration_seconds` con 10 muestras
         (mayoría <1ms) y `projection_lag_seconds` con 9 muestras
         (50-250ms), ambos con datos genuinos de tráfico real, no ceros.
         Confirmado además que sin `METRICS_PORT` (comportamiento default)
         nada cambia — write/read normal sigue funcionando igual.
      2. **NO implementado, roadmap**: clustering de Mosquitto — sigue
         siendo un broker único sin HA, tal como estaba. Es una feature de
         infraestructura de varias semanas (requiere decisión de topología:
         bridge, shared subscription, etc.), no algo para resolver como
         parte de este backlog de viabilidad.
      3. **NO implementado, roadmap**: sharding interno de un store —
         sigue siendo un solo escritor/archivo SQLite por store (DEC-0002,
         por diseño). Implementarlo de verdad es un cambio de arquitectura
         mayor (particionamiento de datos dentro de un store), fuera de
         alcance de una tarea de instrumentación/backpressure.

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

## Active — Investigación de techo de throughput del writer (DEC-0050)

### Próximo ciclo de beta

- [x] Documentación pública y release: README, guía de uso, instalación nativa,
      upgrade, backup/restore y troubleshooting reproducible. La validación
      del libro requiere instalar `mdbook` en el entorno del agente.
- [ ] `TASK-VAL-0033` — Forzar de forma determinista el crash entre PUBACK y
      `published_at`; verificar que no haya pérdida y cuantificar duplicados.
- [ ] `TASK-VAL-0034` — Alertas operativas para writer detenido, último commit,
      cola de consumo, outbox, errores MQTT y lag de proyecciones.
- [ ] `TASK-VAL-0035` — Investigar y documentar el atasco de sesión MQTT bajo
      sobrecarga extrema; definir recuperación automática sólo con evidencia.

- [x] `TASK-VAL-0018` — Investigar por qué el writer solo compromete ~30-40
      comandos/s a SQLite bajo carga sostenida (hallado en DEC-0049). 8
      hipótesis probadas con datos reales (ver tabla completa en DEC-0050):
      `fsync`/`synchronous=NORMAL` (sin cambio), `cache_sync` secuencial
      (sin cambio), CPU insuficiente 2→6 vCPU (sin cambio), entorno
      emulado→Debian real nativo amd64 (sin cambio), timer recreado por
      iteración→`interval` persistente (sin cambio), contención de lock con
      `event_publisher` (descartada, 1000/1000 commits/s en aislamiento),
      mecánica del canal/`select!` (descartada, 1099.9 commits/s en
      aislamiento). Se corrigieron 2 bugs reales de rendimiento:
      `cache_sync.rs` sin `block_in_place` innecesario + despacho
      concurrente, y `WriteEngine::process_batch` movido a
      `tokio::task::spawn_blocking` (no estaba, mismo anti-patrón que
      `cache_server.rs`/DEC-0045) — mejora medida de ~13% (35.64→40.11
      commits/s). Se crearon 3 benchmarks aislados reusables
      (`crates/ixmati-writer/examples/bench_disk.rs`,
      `bench_contention.rs`, `bench_channel_loop.rs`) y 2 métricas nuevas
      (`CACHE_SYNC_DURATION`, `BATCH_FILL_DURATION`). **Resultado**: el
      41% del ciclo de cada batch sigue sin explicarse con instrumentación
      de aplicación — probable overhead de scheduling del runtime de tokio
      bajo contención real, no reproducible en benchmarks aislados.
- [x] `TASK-VAL-0021` (intentado, REVERTIDO) — Hilo de sistema operativo
      dedicado (`WriteActor`) en vez de `spawn_blocking` por llamada.
      **158.35 commits/s en ráfaga (~4x sobre spawn_blocking)**, pero el
      writer dejaba de procesar batches silenciosamente después de ~90-95
      batches bajo carga sostenida de 3 min — sin error, sin panic, sin
      reinicio de systemd. Revertido por seguridad: preferible más lento
      pero confiable (spawn_blocking) que más rápido pero con cuelgues
      silenciosos. Ver DEC-0051 para la implementación completa
      documentada (por si se retoma) y la evidencia del cuelgue.
- [x] `TASK-VAL-0022` (investigación del cuelgue, sin causa raíz confirmada)
      — `strace` no funcionable en el contenedor (ptrace bloqueado por
      política anidada del host). Con `/proc/<pid>/task/*/wchan` se
      confirmó que los 12 hilos del proceso estaban dormidos (ninguno
      atascado en I/O) durante el cuelgue — descarta el deadlock de
      contención SQLite que se sospechaba. Mosquitto mostraba el cliente
      del writer conectado pero sin recibir mensajes nuevos
      (`messages/stored` en el tope). Causa raíz no confirmada — candidato
      más probable: pérdida de wakeup entre el eventloop de `rumqttc` y el
      resto del pipeline. Ver DEC-0051.
- [x] `TASK-VAL-0019` — `event_publisher.rs` tenía el mismo anti-patrón que
      `process_batch` (conexiones SQLite síncronas dentro de `async fn`, sin
      `spawn_blocking`). Confirmado el mecanismo con una prueba unitaria
      pura (`event_publisher::tests::worker_starvation`, sin Debian/carga
      real): con `worker_threads=1`, una llamada bloqueante sin
      `spawn_blocking` congela otras tareas del runtime (≤3 ticks en
      300ms); envuelta en `spawn_blocking`, no las afecta (>100 ticks).
      Corregido envolviendo las 4 llamadas en 2 `tokio::task::spawn_blocking`
      (apertura+fetch, apertura+mark_published_batch), mismo patrón que
      Opción H (DEC-0050). `cargo test --workspace --lib` en verde, sin
      regresiones. Ver DEC-0052. **Pendiente**: revalidar en Debian real con
      `wrk` para medir cuánto del 41% no explicado de DEC-0050 cierra esto
      — no se hizo en esta sesión (sin acceso a la infra remota).
- [x] `DEC-0052`/`DEC-0053` — Motor de escritura 100% síncrono para
      `ixmati-writer`, implementado: sin tokio en el proceso (`main.rs` es
      `fn main()` normal), hilo de SO dedicado dueño exclusivo de SQLite
      (`write_thread.rs`, comunicación 100% `std::sync::mpsc`, sin
      `tokio::sync::oneshot`), MQTT vía `rumqttc::Client` síncrono
      (`consumer.rs`/`event_publisher.rs`, cada uno en su propio hilo),
      cliente de cache-server síncrono nuevo (`ixmati-cache::SyncCacheClient`,
      mismo protocolo de texto que el async, verificado con un servidor de
      juguete en tests). `cargo test --workspace --lib` en verde (8/8
      crates). **Smoke test real** (Mosquitto + cache-server + writer, los 3
      binarios reales en local): pipeline completo verificado
      (MQTT→batch→SQLite→cache→outbox→MQTT). **Carga sostenida de 70s**:
      3,030 batches, 15,150 comandos comprometidos, 0 errores, outbox
      drenado a 0 al final, sin ningún signo del cuelgue silencioso de
      `WriteActor` (DEC-0051, que se congelaba a los ~90-95 batches). Ver
      DEC-0053. Ver actualización abajo — **ya validado en el contenedor
      Debian real de producción**.
- [x] Validar el motor síncrono (DEC-0053) en el contenedor Debian real de
      producción (mismo host amd64 remoto de toda la investigación previa,
      systemd real, `--privileged`). **Instalador completo 7/7 en verde**
      (de paso se encontró y corrigió un bug preexistente no relacionado:
      `systemd/ixmati-api.service` nunca seteaba `SQLITE_PATH`, rompía
      `ack_mode: committed` desde DEC-0049). **Carga sostenida de 3 min con
      `wrk`** (mismo script/metodología de DEC-0046/0049): 464,474 requests
      aceptadas, 0 errores. Lado del writer: 6,500 comandos comprometidos
      (confirmado en SQLite real), 0 reinicios, 0 errores/panics, memoria
      estable, outbox drenado a 0. **Hallazgo honesto**: el throughput NO
      mejoró — 36.1 commits/s, dentro del mismo rango de ~30-40/s medido en
      DEC-0049/0050 con la implementación anterior. El motor síncrono
      resuelve la fragilidad arquitectónica (elimina la clase de bug
      async/sync, evita el cuelgue de `WriteActor`) pero no el techo de
      comandos/s — consistente con que el costo dominante sea el
      commit/fsync de SQLite en sí, no el scheduling de tokio. El 41% de
      ciclo sin explicar de DEC-0050 sigue sin cerrar. Ver DEC-0053
      (actualización).
- [x] `TASK-VAL-0020` — Recalibrados a la capacidad real medida:
      `MAX_WRITES_PER_WINDOW` 1000→**40**/s, `OUTBOX_BACKPRESSURE_THRESHOLD`
      5000→**500** (`crates/ixmati-api/src/rest.rs`, ahora constantes
      `DEFAULT_*` con test de regresión). **Hallazgo de código**:
      `OutboxBacklog`/`self_monitor.rs` mide `_outbox WHERE published_at IS
      NULL` (filas ya comprometidas, esperando publicarse) — NO la cola de
      ingestión (comandos aceptados, aún sin comprometer). Por eso en la
      carga de DEC-0053 (464K aceptadas, throttle deshabilitado) el
      backpressure de outbox nunca se disparó ni un solo 429: es ciego a
      ese modo de falla concreto. El mecanismo que sí puede actuar a
      tiempo es el rate-limiter, ahora calibrado cerca del límite real.
      **Validado en el mismo contenedor Debian real de DEC-0053, sin
      overrides esta vez**: carga de 1 min con `wrk` → 134,544 requests,
      132,144 rechazadas con backpressure, solo 2,400 aceptadas (≈40/s
      exacto). Lado del writer: **2,400 comprometidos — igual a las 2,400
      aceptadas**, por primera vez en esta investigación `aceptadas ==
      comprometidas`, sin backlog oculto. 0 reinicios, 0 errores. También
      cierra `TASK-VAL-0013` (el default ahora es visible/documentado en
      `systemd/ixmati-api.service`). Ver DEC-0054.
- [x] `TASK-VAL-0023` — Corrección de métricas (filas vs. commits reales),
      descomposición del ciclo con datos (61.3% sin explicar — hipótesis
      del `cache_sync` secuencial REFUTADA, solo 10.3% del ciclo), y
      primera latencia end-to-end honesta con `ack_mode: committed`
      (p50=14.55ms, p90=121.67ms, p99=185.20ms). Ver DEC-0055.
- [x] `TASK-VAL-0024` P0 — `client_id` estable
      (`ixmati-writer-{store}`, antes UUID aleatorio) +
      `set_clean_session(false)` en `consumer.rs`/`event_publisher.rs` +
      `persistent_client_expiration 7d` en `mosquitto.conf` +
      contador `mqtt_ack_failures_total` nuevo. **Validado en el mismo
      contenedor Debian real, mismo escenario de sobrecarga de DEC-0055**:
      al reiniciar el writer tras saturar la cola de Mosquitto, esta vez
      **sí recuperó backlog** (63→114 batches, Mosquitto bajó de 100,097 a
      94,997 mensajes) — antes: 0 recuperado, pérdida total. **Pero el
      atasco volvió a ocurrir a mitad del drenado** (0% CPU real
      confirmado, cola de Mosquitto sin bajar en 2 muestras separadas 3s)
      — el fix reduce el blast radius de un reinicio, no corrige la causa
      de que la sesión se atasque. Ver DEC-0056.
- [x] `TASK-VAL-0024` P1 (**hipótesis de causa raíz REFUTADA — atasco
      persiste**) — `client.ack()` → `client.try_ack()` (no bloqueante) en
      `consumer.rs` + `max_inflight_messages` 20→200 en `mosquitto.conf`.
      **Validado con el mismo escenario, esta vez SIN reiniciar el writer,
      observando 2 minutos completos**: el atasco ocurrió de todos modos
      (51 batches, luego 0% CPU real y Mosquitto congelado en 100,453
      mensajes durante 8 muestras consecutivas a lo largo de 120s). La
      hipótesis de que `client.ack()` bloqueante + `max_inflight_messages`
      bajo explicaban el atasco queda refutada por evidencia directa — la
      causa raíz real sigue sin identificarse. El fix se mantiene igual
      (es una mejora de robustez legítima por sí sola), pero no cierra el
      hallazgo. Siguiente paso: `strace`/`gdb` reales o logs de Mosquitto
      en nivel `debug` — no disponibles en esta sesión. Ver DEC-0057.
- [x] `TASK-VAL-0026` (P1.5) — Escalera de carga con `ack_mode: committed`
      (`helpers/wrk/staircase.sh`, nuevo) para reemplazar "~36-40
      commits/s" por throughput sostenible real. **Válido en 20-100/s**
      (aceptadas/s coincide exacto con el objetivo en cada escalón): `p50`
      estable y bajo en todo el rango (17.85-20.96ms), pero `p90`/`p99`
      se degradan visiblemente desde 60/s (`p90` 37ms→109ms) y ya
      pronunciado en 100/s (`p90`=216ms, `p99`=320ms) — sin que
      `outbox_size`/`consumer_queue_depth` muestren crecimiento
      descontrolado (`outbox_size` aparece en 48 desde 100/s pero no
      sigue subiendo). El rate-limiter de producción (40/s, DEC-0054)
      queda en zona saludable. **Los escalones 150/200 NO son datos
      confiables** — la concurrencia fija de `wrk` (`-c50`) se vuelve el
      límite antes que el rate-limiter una vez que la latencia sube lo
      suficiente (150/s objetivo → solo 136/s real; 200/s objetivo → 115/s
      real, con 0 rechazos porque el límite nunca se activó). El punto
      real de quiebre (cola creciendo sin límite) no se encontró — hace
      falta repetir con mayor concurrencia o una herramienta con control
      de tasa real (`wrk2`/`vegeta`). Ver DEC-0058.
- [ ] `TASK-VAL-0025` — El 61.3% del ciclo por batch sin explicar
      (DEC-0055, empeora el 41% de DEC-0050) sigue abierto. Candidato no
      probado: `tracing::info!` con formato JSON hace I/O de escritura
      síncrona por línea de log, que bajo journald puede bloquear.
      Instrumentar el propio loop de `main.rs` con mediciones más finas
      (tiempo entre que `process_batch()` retorna y el próximo
      `fill_started` se resetea) para aislarlo.
- [x] `TASK-VAL-0027` (P0) — ACK MQTT del consumidor sólo después del commit
      SQLite: `PendingCommand` conserva el token de ACK hasta `process_batch`
      exitoso; la cola acotada deja de ser una falsa frontera de durabilidad.
      La API convierte `accepted` en alias durable y sólo devuelve `200` tras
      confirmar `_idempotency`; ver DEC-0059.
- [x] `TASK-VAL-0028` (P0) — Outbox sólo se marca publicado después de PUBACK:
      `EventPublisher` correlaciona `Outgoing::Publish(pkid)` con el id de
      fila y espera `Incoming::PubAck` antes de `mark_published_batch`. La
      semántica resultante es at-least-once, sin pérdida en un crash.
- [x] `TASK-VAL-0029` (P1) — Observabilidad y metodología de carga: profundidad
      de cola, comandos encolados/deferidos/confirmados, último commit y
      errores de cache instrumentados; `staircase.sh` reporta `NA` para series
      ausentes y usa `wrk2` o `helpers/python/rate_load.py` rate-controlled.
- [x] `TASK-VAL-0030` — Validación final en `main` (`6eaa80a`) contra Debian
      amd64: instalación idempotente desde el tarball, cinco servicios activos,
      `/health` OK y escalera con tasa global exacta a 20/40/60/80/100/s.
      Resultado: p50=145/605/767/892/919ms y p99=283/2052/1858/1601/1550ms;
      150/200 quedaron limitados por concurrencia del generador (119/100
      requests/s observadas) y se marcan inconclusos para capacidad del
      servidor. Ver DEC-0060.
- [x] `TASK-VAL-0031` — Crash/restart durante ingestión MQTT: 30/30 claves
      confirmadas por API y SQLite, 30/30 eventos observados en MQTT después
      de `SIGKILL` y reinicio systemd, sin pérdida observada. El intervalo
      PUBACK→marca del outbox sigue siendo at-least-once y no se forzó de forma
      determinista en esta corrida. Ver DEC-0060.
- [x] `TASK-VAL-0032` — Concurrencia productiva: el batcher ahora respeta
      `BATCH_INTERVAL_MS` incluso con tráfico continuo; la espera de commit
      SQLite del API sale del worker async. En Debian amd64, el SHA `9d96b0b`
      sostuvo 40/60/80/100/120 solicitudes/s rate-controlled con `200` y
      `consumer_queue_depth=0`; 150/s fue el primer escalón con `202` y cola.
      Ver DEC-0061.

## Cancelled / Replaced

- [x] `TASK-WRITE-0002` — Spike comparativa Opción A vs B (cerrada por diseño)
- [x] `TASK-WRITE-0013` — ixmati-resync (reemplazada por reconciler fan-in)

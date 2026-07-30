# ROADMAP.md

Prioridades de mediano plazo para Ixmati.

## Fase T — Tooling y harness (prerrequisito)

**Objetivo**: infraestructura de desarrollo antes de escribir código del motor. Bloquea todas las fases siguientes.

- `TASK-TOOL-0001`..`TASK-TOOL-0014`: Makefile, Justfile, helpers, hooks, workspace, tests, coverage, docs, CI.
- **Dependencia bloqueante**: `SPEC-WRITE-0001` no puede iniciar implementación hasta que `just test-unit` pase de rojo a verde.

**Duración estimada**: ya completada (scaffolding). Solo falta instalar Rust y corregir los 7 tests bootstrap.

## Fase 0 — Contratos y decisión técnica

**Objetivo**: cerrar el riesgo FlashDB (DEC-0009) y definir todos los contratos antes de escribir código de producción. Las decisiones de arquitectura (A vs B, lectura directa) ya están cerradas.

- `TASK-WRITE-0001`: spike de viabilidad de FlashDB vía FFI en Rust.
- `TASK-WRITE-0003`: definir contrato de envelope (comando + evento), topics MQTT (cmd + evt) y `.proto`.
- `TASK-WRITE-0004`: definir contrato de API REST (OpenAPI).

**Duración estimada**: 1-2 semanas.
**Tarea cancelada**: `TASK-WRITE-0002` (comparativa A/B). Las decisiones DEC-0010 y DEC-0011 se cerraron por diseño.

## Fase 1 — Core, store registry y writer

**Objetivo**: el writer procesa comandos con outbox transaccional. Múltiples stores independientes. `stores=1` funciona sin overhead de eventos.

- `TASK-WRITE-0005`: implementar `ixmati-core` (tipos, envelopes, errores, config, StoreConfig, lógica de proyección compartida).
- `TASK-WRITE-0017`: store registry + config multi-store + resolución de topología (N pods vs 1 proceso).
- `TASK-WRITE-0006`: implementar `ixmati-writer` (consumer MQTT, batching, dedup, `BEGIN IMMEDIATE`).
- `TASK-WRITE-0018`: tabla `_outbox`, inserción transaccional, publicador como task del writer.
- `TASK-WRITE-0007`: tests de crash del writer (kill -9, 0 comandos perdidos, 0 eventos perdidos, orden).

**Duración estimada**: 3-4 semanas.
**Tarea cancelada**: `TASK-WRITE-0013` (resync mono-store), reemplazada por `TASK-WRITE-0022` (reconciler fan-in).

## Fase 2 — API REST/gRPC

**Objetivo**: los backends envían comandos y consultan estado vía API HTTP y gRPC.

- `TASK-WRITE-0008`: implementar `ixmati-api` (axum REST + tonic gRPC).
- `TASK-WRITE-0009`: implementar modo async (`ack=accepted`) y modo sync (`ack=committed`) con correlación.
- `TASK-WRITE-0010`: endpoint `GET /writes/{store}/{idempotency_key}`.

**Duración estimada**: 2-3 semanas.

## Fase 3 — Cache, eventos y proyecciones

**Objetivo**: las lecturas se sirven desde cache-aside (por defecto) y desde read models proyectados (opt-in). El bus de eventos alimenta los proyectores.

- `TASK-WRITE-0011`: implementar `ixmati-cache` con trait `CacheBackend` y namespaces `c:` / `p:`.
- `TASK-WRITE-0012`: invalidación/repoblación de cache-aside desde el writer tras cada commit.
- `TASK-WRITE-0019`: `EventEnvelope` + publicación en `ixmati/evt/...`.
- `TASK-WRITE-0020`: `ixmati-projector`: runtime de proyecciones idempotentes (patrón R y M).
- `TASK-WRITE-0021`: declaración de proyecciones en config + validación (fan-out, stores fuente).

**Duración estimada**: 3-4 semanas.

## Fase 4 — Reconciler, disaster recovery y producción

**Objetivo**: el sistema tolera fallos, los read models son reconstruibles, y hay runbook operativo.

- `TASK-WRITE-0022`: `ixmati-reconciler` (reproyección fan-in sobre N stores).
- `TASK-WRITE-0014`: configurar Litestream con ≥2 destinos por store.
- `TASK-WRITE-0023`: ATTACH DATABASE read-only para reporting cross-store.
- `TASK-WRITE-0015`: health checks integrados (API, writer por store, Mosquitto, SQLite, cache).
- `TASK-WRITE-0024`: `ixmati-supervisor` + K8s manifests (Deployment, PVC, sidecar por store).
- `TASK-WRITE-0016`: documentar runbook de producción (restore por store, failover, reproyección, troubleshooting).

**Duración estimada**: 2-3 semanas.

## Fase 5 — Observabilidad, consistencia y hardening

**Objetivo**: visibilidad operativa, validación de garantías de consistencia, y benchmarks.

- `TASK-WRITE-0025`: tests de consistencia eventual (outbox, idempotencia de proyecciones, lag, reproyección).
- Métricas Prometheus (lag de cola, lag de proyección, latencia de commit, tasa de misses, tamaño de outbox, eventos publicados/segundo).
- Backpressure: rechazar comandos cuando la cola supera un umbral por store.
- Alertas: writer caído, cola creciendo, outbox estancado, Litestream detenido, SQLite corrupto, lag de proyección > 1s.
- Benchmarks de throughput multi-store (comandos/segundo, lecturas/segundo) y límites operativos documentados.

**Duración estimada**: 2 semanas.

## Más allá (backlog no priorizado)

- Writer standby con failover automático por store.
- Soporte para partición interna de un store (sharding por carga, no por dominio).
- Plugin de autenticación en la API (JWT, API keys).
- Dashboard web de operación (lag, throughput, errores por store).
- CDC para suscriptores externos vía MQTT (reutilizando el bus de eventos).
- Migración de stores (renombrar, merge, split).
- Soporte para múltiples brokers Mosquitto (clustering).

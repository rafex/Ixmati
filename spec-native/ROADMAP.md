# ROADMAP.md

Prioridades de mediano plazo para Ixmati.

## Fase 0 — Spike de decisión y contratos (actual)

**Objetivo**: cerrar las decisiones abiertas (DEC-0010, DEC-0011) y definir los contratos antes de escribir código de producción.

- `TASK-WRITE-0001`: spike de viabilidad de FlashDB vía FFI en Rust.
- `TASK-WRITE-0002`: spike comparativo Opción A vs Opción B (durabilidad, latencia, orden).
- `TASK-WRITE-0003`: definir contrato de envelope, topics MQTT y `.proto` para gRPC.
- `TASK-WRITE-0004`: definir contrato de API REST (OpenAPI).

**Duración estimada**: 1-2 semanas.

## Fase 1 — Writer + SQLite (core)

**Objetivo**: el writer es funcional. Acepta mensajes, aplica escrituras en SQLite con batching y deduplicación.

- `TASK-WRITE-0005`: implementar `ixmati-core` (tipos, envelope, errores, config).
- `TASK-WRITE-0006`: implementar `ixmati-writer` con consumer MQTT, lógica de `BEGIN IMMEDIATE`, batching, dedup.
- `TASK-WRITE-0007`: tests de crash del writer (kill -9, 0 pérdidas, orden preservado).

**Duración estimada**: 2-3 semanas.

## Fase 2 — API REST/gRPC

**Objetivo**: los backends pueden enviar escrituras y consultar estado vía API HTTP y gRPC.

- `TASK-WRITE-0008`: implementar `ixmati-api` (axum REST + tonic gRPC).
- `TASK-WRITE-0009`: implementar modo async (`ack=accepted`) y modo sync (`ack=committed`) con correlación de respuestas.
- `TASK-WRITE-0010`: endpoint `GET /writes/{idempotency_key}` para consulta de estado.

**Duración estimada**: 2-3 semanas.

## Fase 3 — Cache y resync

**Objetivo**: las lecturas se sirven desde FlashDB (o alternativa) con fallback a SQLite.

- `TASK-WRITE-0011`: implementar `ixmati-cache` con trait `CacheBackend` e implementación FlashDB (o alternativa según spike).
- `TASK-WRITE-0012`: invalidación/repoblación de cache desde el writer tras cada commit.
- `TASK-WRITE-0013`: implementar `ixmati-resync` (reconstrucción offline de cache desde SQLite).

**Duración estimada**: 2 semanas.

## Fase 4 — Disaster recovery y producción

**Objetivo**: el sistema tolera fallos y se puede recuperar desde backup.

- `TASK-WRITE-0014`: configurar Litestream con ≥2 destinos de backup.
- `TASK-WRITE-0015`: health checks integrados (API, writer, Mosquitto, SQLite, cache).
- `TASK-WRITE-0016`: documentar runbook de producción (restore, failover, resync).

**Duración estimada**: 1-2 semanas.

## Fase 5 — Observabilidad y producción avanzada

**Objetivo**: visibilidad operativa y hardening.

- Métricas Prometheus (lag de cola, latencia de commit, tasa de misses, tamaño de cache).
- Backpressure: rechazar escrituras cuando la cola supera un umbral.
- Alertas: writer caído, cola creciendo, Litestream detenido, SQLite corrupto.
- Benchmarks de throughput (escrituras/segundo, lecturas/segundo) y límites operativos documentados.

**Duración estimada**: 2 semanas.

## Más allá (backlog no priorizado)

- Writer standby con failover automático.
- Soporte para múltiples archivos SQLite (sharding por entidad).
- Plugin de autenticación en la API (JWT, API keys).
- Dashboard web de operación (lag, throughput, errores).
- CDC (change data capture) para suscriptores externos vía MQTT.

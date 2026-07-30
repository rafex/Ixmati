# TODO.md

Tablero de tareas activo. Persiste entre sesiones.

## Active — Tooling (SPEC-TOOL-0001)

- [ ] `TASK-TOOL-0013` — Rellenar spec-native/pipelines/CI.md y CD.md con gates reales

## Active — Write Engine (SPEC-WRITE-0001)

- [ ] `TASK-WRITE-0001` — Spike: viabilidad de FlashDB vía FFI en Rust
- [ ] `TASK-WRITE-0003` — Definir contrato de envelope, topics MQTT y .proto
- [ ] `TASK-WRITE-0004` — Definir contrato de API REST (OpenAPI)
- [ ] `TASK-WRITE-0005` — Implementar ixmati-core
- [ ] `TASK-WRITE-0006` — Implementar ixmati-writer
- [ ] `TASK-WRITE-0007` — Tests de crash del writer
- [ ] `TASK-WRITE-0008` — Implementar ixmati-api
- [ ] `TASK-WRITE-0009` — Implementar modo async y sync
- [ ] `TASK-WRITE-0010` — Endpoint GET /writes/{store}/{idempotency_key}
- [ ] `TASK-WRITE-0011` — Implementar ixmati-cache
- [ ] `TASK-WRITE-0012` — Invalidación/repoblación de cache-aside
- [ ] `TASK-WRITE-0014` — Configurar Litestream por store
- [ ] `TASK-WRITE-0015` — Health checks integrados
- [ ] `TASK-WRITE-0016` — Documentar runbook de producción
- [ ] `TASK-WRITE-0017` — Store registry + config multi-store
- [ ] `TASK-WRITE-0018` — Tabla _outbox + publicador transaccional
- [ ] `TASK-WRITE-0019` — EventEnvelope + bus de eventos
- [ ] `TASK-WRITE-0020` — ixmati-projector
- [ ] `TASK-WRITE-0021` — Declaración de proyecciones en config
- [ ] `TASK-WRITE-0022` — ixmati-reconciler
- [ ] `TASK-WRITE-0023` — ATTACH read-only
- [ ] `TASK-WRITE-0024` — ixmati-supervisor + K8s manifests
- [ ] `TASK-WRITE-0025` — Tests de consistencia eventual

## Cancelled / Replaced

- [x] `TASK-WRITE-0002` — Spike comparativa Opción A vs B (cerrada por diseño)
- [x] `TASK-WRITE-0013` — ixmati-resync (reemplazada por reconciler fan-in)

## Done (Tooling)

- [x] `TASK-TOOL-0001`..`TASK-TOOL-0012`, `TASK-TOOL-0014` — Scaffolding completo

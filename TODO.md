# TODO.md

Tablero de tareas activo. Persiste entre sesiones.

## Active

- [ ] `TASK-WRITE-0001` — Spike: viabilidad de FlashDB vía FFI en Rust
- [ ] `TASK-WRITE-0003` — Definir contrato de envelope (comando + evento), topics MQTT y .proto
- [ ] `TASK-WRITE-0004` — Definir contrato de API REST (OpenAPI)
- [ ] `TASK-WRITE-0005` — Implementar ixmati-core (tipos, envelopes, errores, config, StoreConfig)
- [ ] `TASK-WRITE-0006` — Implementar ixmati-writer (consumer, batching, dedup, outbox transaccional)
- [ ] `TASK-WRITE-0007` — Tests de crash del writer (kill -9, 0 comandos perdidos, 0 eventos perdidos)
- [ ] `TASK-WRITE-0008` — Implementar ixmati-api (REST + gRPC)
- [ ] `TASK-WRITE-0009` — Implementar modo async y sync con correlación
- [ ] `TASK-WRITE-0010` — Endpoint GET /writes/{store}/{idempotency_key}
- [ ] `TASK-WRITE-0011` — Implementar ixmati-cache (trait CacheBackend, namespaces c: y p:)
- [ ] `TASK-WRITE-0012` — Invalidación/repoblación de cache-aside desde el writer
- [ ] `TASK-WRITE-0014` — Configurar Litestream por store con ≥2 destinos
- [ ] `TASK-WRITE-0015` — Health checks integrados (por store)
- [ ] `TASK-WRITE-0016` — Documentar runbook de producción
- [ ] `TASK-WRITE-0017` — Store registry + config multi-store
- [ ] `TASK-WRITE-0018` — Tabla _outbox + publicador transaccional
- [ ] `TASK-WRITE-0019` — EventEnvelope + bus de eventos (ixmati/evt/...)
- [ ] `TASK-WRITE-0020` — ixmati-projector: runtime de proyecciones idempotentes
- [ ] `TASK-WRITE-0021` — Declaración de proyecciones en config
- [ ] `TASK-WRITE-0022` — ixmati-reconciler: reproyección fan-in sobre N stores
- [ ] `TASK-WRITE-0023` — ATTACH read-only para reporting cross-store
- [ ] `TASK-WRITE-0024` — ixmati-supervisor + K8s manifests
- [ ] `TASK-WRITE-0025` — Tests de consistencia eventual

## Cancelled

- [x] `TASK-WRITE-0002` — Spike comparativa Opción A vs B (cerrada por diseño)
- [x] `TASK-WRITE-0013` — ixmati-resync (reemplazada por reconciler fan-in)

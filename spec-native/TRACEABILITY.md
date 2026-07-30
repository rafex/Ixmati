# TRACEABILITY.md

Vínculos entre artefactos del proyecto.

## Especificaciones

| Spec | Estado | Owner | Tareas | Decisiones |
|---|---|---|---|---|
| `SPEC-AUTH-0001` | `active` | team-auth | `TASK-AUTH-0001`..`TASK-AUTH-0003` | — |
| `SPEC-WRITE-0001` | `active` | team-core | `TASK-WRITE-0001`, `0003`..`0012`, `0014`..`0025` | `DEC-0001`..`DEC-0020` |

## Decisiones

| Decisión | Estado | Specs | Tareas |
|---|---|---|---|
| `DEC-0001` — SQLite WAL + sync=NORMAL por store | `accepted` | `SPEC-WRITE-0001` | `0005`, `0006`, `0017` |
| `DEC-0002` — Un solo escritor por store | `accepted` | `SPEC-WRITE-0001` | `0006`, `0014` |
| `DEC-0003` — Idempotencia con version | `accepted` | `SPEC-WRITE-0001` | `0005`, `0006` |
| `DEC-0004` — Particionado store/entity/id | `accepted` | `SPEC-WRITE-0001` | `0006`, `0017` |
| `DEC-0005` — Batching BEGIN IMMEDIATE | `accepted` | `SPEC-WRITE-0001` | `0006` |
| `DEC-0006` — Cache reconstruible | `superseded` | — | — |
| `DEC-0007` — Litestream ≥2 destinos por store | `accepted` | `SPEC-WRITE-0001` | `0014`, `0024` |
| `DEC-0008` — Async/sync seleccionable, 1 comando = 1 store | `accepted` | `SPEC-WRITE-0001` | `0009`, `0010` |
| `DEC-0009` — Riesgo FlashDB FFI (severidad alta) | `accepted` | `SPEC-WRITE-0001` | `0001` |
| `DEC-0010` — Canal de ingesta: Mosquitto (Opción A) | `accepted` | `SPEC-WRITE-0001` | `0006`, `0017` |
| `DEC-0011` — Lectura directa | `accepted` | `SPEC-WRITE-0001` | `0011`, `0020` |
| `DEC-0012` — Store como primitivo del motor | `accepted` | `SPEC-WRITE-0001` | `0017` |
| `DEC-0013` — Sin transacciones cross-store | `accepted` | `SPEC-WRITE-0001` | `0005` |
| `DEC-0014` — Transactional outbox | `accepted` | `SPEC-WRITE-0001` | `0018` |
| `DEC-0015` — Taxonomía cmd vs evt | `accepted` | `SPEC-WRITE-0001` | `0003`, `0019` |
| `DEC-0016` — Read models: R por defecto, M por excepción | `accepted` | `SPEC-WRITE-0001` | `0020`, `0021` |
| `DEC-0017` — ATTACH read-only | `accepted` | `SPEC-WRITE-0001` | `0023` |
| `DEC-0018` — Keyspace unificado c: / p: | `accepted` | `SPEC-WRITE-0001` | `0011`, `0020` |
| `DEC-0019` — Reconciler fan-in | `accepted` | `SPEC-WRITE-0001` | `0022` |
| `DEC-0020` — Coexistencia cache-aside + proyecciones | `accepted` | `SPEC-WRITE-0001` | `0011`, `0020`, `0021` |

## Tareas

| Tarea | Estado | Spec | Dependencias | Validación |
|---|---|---|---|---|
| `TASK-WRITE-0001` | `todo` | `SPEC-WRITE-0001` | — | Compilación Linux + benchmark + TTL |
| `TASK-WRITE-0002` | `cancelled` | — | — | Cancelada (DEC-0010/0011 cerradas por diseño) |
| `TASK-WRITE-0003` | `todo` | `SPEC-WRITE-0001` | — | protoc + taxonomía cmd/evt |
| `TASK-WRITE-0004` | `todo` | `SPEC-WRITE-0001` | `0003` | swagger-cli validate |
| `TASK-WRITE-0005` | `todo` | `SPEC-WRITE-0001` | `0003`, `0004` | cargo build + tests envelope |
| `TASK-WRITE-0006` | `todo` | `SPEC-WRITE-0001` | `0005` | Tests batch, dedup, outbox |
| `TASK-WRITE-0007` | `todo` | `SPEC-WRITE-0001` | `0006`, `0018` | crash_test.sh N=1000, multi-store |
| `TASK-WRITE-0008` | `todo` | `SPEC-WRITE-0001` | `0003`, `0004`, `0005` | Tests REST + gRPC |
| `TASK-WRITE-0009` | `todo` | `SPEC-WRITE-0001` | `0008`, `0006` | Tests async/sync |
| `TASK-WRITE-0010` | `todo` | `SPEC-WRITE-0001` | `0009` | Test GET status por store |
| `TASK-WRITE-0011` | `todo` | `SPEC-WRITE-0001` | `0001`, `0005` | Tests get/set/invalidate + namespaces |
| `TASK-WRITE-0012` | `todo` | `SPEC-WRITE-0001` | `0006`, `0011` | Test invalidación c:* |
| `TASK-WRITE-0013` | `cancelled` | — | — | Reemplazada por `0022` |
| `TASK-WRITE-0014` | `todo` | `SPEC-WRITE-0001` | `0006`, `0017` | Test restore Litestream por store |
| `TASK-WRITE-0015` | `todo` | `SPEC-WRITE-0001` | `0008`, `0006`, `0011` | Health check por store |
| `TASK-WRITE-0016` | `todo` | `SPEC-WRITE-0001` | `0014`, `0015`, `0022` | Walkthrough manual |
| `TASK-WRITE-0017` | `todo` | `SPEC-WRITE-0001` | `0005` | Config multi-store, stores=1 sin overhead |
| `TASK-WRITE-0018` | `todo` | `SPEC-WRITE-0001` | `0006` | Outbox transaccional, 0 eventos perdidos en crash |
| `TASK-WRITE-0019` | `todo` | `SPEC-WRITE-0001` | `0003`, `0018` | EventEnvelope + publicación en evt |
| `TASK-WRITE-0020` | `todo` | `SPEC-WRITE-0001` | `0011`, `0019` | Proyector idempotente, R y M |
| `TASK-WRITE-0021` | `todo` | `SPEC-WRITE-0001` | `0020` | Config + validación de proyecciones |
| `TASK-WRITE-0022` | `todo` | `SPEC-WRITE-0001` | `0011`, `0021` | Reconciler fan-in, total y selectivo |
| `TASK-WRITE-0023` | `todo` | `SPEC-WRITE-0001` | `0006`, `0017` | ATTACH read-only, bloqueado en write |
| `TASK-WRITE-0024` | `todo` | `SPEC-WRITE-0001` | `0017`, `0014` | Supervisor + K8s manifests |
| `TASK-WRITE-0025` | `todo` | `SPEC-WRITE-0001` | `0018`, `0020`, `0022` | Outbox crash, idempotencia, lag, reproyección |

## Artefactos por iniciativa

### write-engine

- **Spec**: `spec-native/specs/write-engine/SPEC.md`
- **Tareas**: `spec-native/tasks/write-engine/TASKS.md`
- **Crates esperados**: `ixmati-core`, `ixmati-api`, `ixmati-writer`, `ixmati-cache`, `ixmati-projector`, `ixmati-reconciler`, `ixmati-supervisor`
- **Archivos de configuración**: `proto/*`, `config/*`, `docker/*`, `k8s/*`

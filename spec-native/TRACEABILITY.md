# TRACEABILITY.md

Vínculos entre artefactos del proyecto.

## Especificaciones

| Spec | Estado | Owner | Tareas | Decisiones |
|---|---|---|---|---|
| `SPEC-AUTH-0001` | `active` | team-auth | `TASK-AUTH-0001`..`0003` | — |
| `SPEC-WRITE-0001` | `active` | team-core | `TASK-WRITE-0001`, `0003`..`0012`, `0014`..`0025` | `DEC-0001`..`DEC-0020` |
| `SPEC-TOOL-0001` | `active` | team-core | `TASK-TOOL-0001`..`0014` | `DEC-0021`..`DEC-0027` |

## Decisiones

| Decisión | Estado | Specs | Tareas |
|---|---|---|---|
| `DEC-0001` — SQLite WAL por store | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0005`, `0006`, `0017` |
| `DEC-0002` — Un escritor por store | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0006`, `0014` |
| `DEC-0003` — Idempotencia | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0005`, `0006` |
| `DEC-0004` — Particionado store/entity/id | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0006`, `0017` |
| `DEC-0005` — Batching BEGIN IMMEDIATE | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0006` |
| `DEC-0006` — Cache reconstruible | `superseded` | — | — |
| `DEC-0007` — Litestream por store | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0014`, `0024` |
| `DEC-0008` — Async/sync | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0009`, `0010` |
| `DEC-0009` — Riesgo FlashDB FFI | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0001` |
| `DEC-0010` — Mosquitto (Opción A) | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0006`, `0017` |
| `DEC-0011` — Lectura directa | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0011`, `0020` |
| `DEC-0012` — Store primitivo | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0017` |
| `DEC-0013` — Sin tx cross-store | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0005` |
| `DEC-0014` — Transactional outbox | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0018` |
| `DEC-0015` — Taxonomía cmd vs evt | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0003`, `0019` |
| `DEC-0016` — Read models R por defecto | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0020`, `0021` |
| `DEC-0017` — ATTACH read-only | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0023` |
| `DEC-0018` — Keyspace unificado | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0011`, `0020` |
| `DEC-0019` — Reconciler fan-in | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0022` |
| `DEC-0020` — Coexistencia cache+proy | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0011`, `0020`, `0021` |
| `DEC-0021` — make=build, just=tasks | `accepted` | `SPEC-TOOL-0001` | `TASK-TOOL-0003..0005` |
| `DEC-0022` — helpers/ fuente única | `accepted` | `SPEC-TOOL-0001` | `TASK-TOOL-0002`, `0004`, `0005` |
| `DEC-0023` — uv + Python ≥3.12 | `accepted` | `SPEC-TOOL-0001` | `TASK-TOOL-0001` |
| `DEC-0024` — Tiers de test | `accepted` | `SPEC-TOOL-0001`, `SPEC-WRITE-0001` | `TASK-TOOL-0007..0009` |
| `DEC-0025` — TDD + ratchet cobertura | `accepted` | `SPEC-TOOL-0001`, `SPEC-WRITE-0001` | `TASK-TOOL-0007`, `0010` |
| `DEC-0026` — Hooks versionados | `accepted` | `SPEC-TOOL-0001` | `TASK-TOOL-0006` |
| `DEC-0027` — docs/ vs spec-native/ | `accepted` | `SPEC-TOOL-0001` | `TASK-TOOL-0012` |

## Tareas

| Iniciativa | Total | Todo | Done | Cancelled |
|---|---|---|---|---|
| `tooling` | 14 | 1 | 13 | 0 |
| `write-engine` | 25 | 21 | 0 | 3 |
| `authentication` | 3 | 1 | 1 | 0 (in_progress=1) |

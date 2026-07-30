# TRACEABILITY.md

Vínculos entre artefactos del proyecto.

## Especificaciones

| Spec | Estado | Owner | Tareas | Decisiones |
|---|---|---|---|---|
| `SPEC-AUTH-0001` | `active` | team-auth | `TASK-AUTH-0001`..`TASK-AUTH-0003` | — |
| `SPEC-WRITE-0001` | `active` | team-core | `TASK-WRITE-0001`..`TASK-WRITE-0016` | `DEC-0001`..`DEC-0011` |

## Decisiones

| Decisión | Estado | Specs relacionadas | Tareas relacionadas |
|---|---|---|---|
| `DEC-0001` — SQLite WAL + sync=NORMAL | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0005`, `TASK-WRITE-0006` |
| `DEC-0002` — Un solo escritor lógico | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0006` |
| `DEC-0003` — Idempotencia con version | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0005`, `TASK-WRITE-0006` |
| `DEC-0004` — Particionado entity+id | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0006` |
| `DEC-0005` — Batching BEGIN IMMEDIATE | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0006` |
| `DEC-0006` — Cache reconstruible | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0011`, `TASK-WRITE-0013` |
| `DEC-0007` — Litestream ≥2 destinos | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0014` |
| `DEC-0008` — Async/sync seleccionable | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0009`, `TASK-WRITE-0010` |
| `DEC-0009` — Riesgo FlashDB FFI | `accepted` | `SPEC-WRITE-0001` | `TASK-WRITE-0001` |
| `DEC-0010` — Canal de ingesta A vs B | `proposed` | `SPEC-WRITE-0001` | `TASK-WRITE-0002` |
| `DEC-0011` — Camino de lectura | `proposed` | `SPEC-WRITE-0001` | `TASK-WRITE-0002` |

## Tareas

| Tarea | Estado | Spec | Dependencias | Validación |
|---|---|---|---|---|
| `TASK-WRITE-0001` | `todo` | `SPEC-WRITE-0001` | — | Compilación Linux + benchmark |
| `TASK-WRITE-0002` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0001` | Crash test, latencia, orden |
| `TASK-WRITE-0003` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0002` | protoc compila sin warnings |
| `TASK-WRITE-0004` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0003` | swagger-cli validate |
| `TASK-WRITE-0005` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0003`, `TASK-WRITE-0004` | cargo build + tests |
| `TASK-WRITE-0006` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0005` | Tests batch, dedup, versiones |
| `TASK-WRITE-0007` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0006` | crash_test.sh con N=1000 |
| `TASK-WRITE-0008` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0003`, `TASK-WRITE-0004`, `TASK-WRITE-0005` | Tests REST + gRPC |
| `TASK-WRITE-0009` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0008`, `TASK-WRITE-0006` | Tests async/sync |
| `TASK-WRITE-0010` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0009` | Test GET status |
| `TASK-WRITE-0011` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0001`, `TASK-WRITE-0005` | Tests get/set/invalidate |
| `TASK-WRITE-0012` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0006`, `TASK-WRITE-0011` | Test invalidación post-commit |
| `TASK-WRITE-0013` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0011` | Test resync 100k registros |
| `TASK-WRITE-0014` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0006` | Test restore Litestream |
| `TASK-WRITE-0015` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0008`, `TASK-WRITE-0006`, `TASK-WRITE-0011` | Health check agregado |
| `TASK-WRITE-0016` | `todo` | `SPEC-WRITE-0001` | `TASK-WRITE-0014`, `TASK-WRITE-0015` | Walkthrough manual |

## Artefactos por iniciativa

### write-engine

- **Spec**: `spec-native/specs/write-engine/SPEC.md`
- **Tareas**: `spec-native/tasks/write-engine/TASKS.md`
- **Crates esperados**: `ixmati-core`, `ixmati-api`, `ixmati-writer`, `ixmati-cache`, `ixmati-resync`
- **Archivos de configuración**: `proto/*`, `config/*`, `docker/*`

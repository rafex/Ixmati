# TRACEABILITY.md

Vínculos entre artefactos del proyecto.

## Especificaciones

| Spec | Estado | Owner | Tareas | Decisiones |
|---|---|---|---|---|
| `SPEC-AUTH-0001` | `active` | team-auth | `TASK-AUTH-0001`..`0003` | — |
| `SPEC-WRITE-0001` | `active` | team-core | 21 todo + 3 cancelled | `DEC-0001`..`DEC-0020` |
| `SPEC-TOOL-0001` | `active` | team-core | 13 done + 1 todo | `DEC-0021`..`DEC-0027` |
| `SPEC-CONTAINERS-0001` | `active` | team-core | 11 done + 1 todo | `DEC-0028`..`DEC-0033` |

## Total

- **Decisiones**: 33 (29 `accepted`, 2 `superseded`, 1 `cancelled`)
- **Tareas**: 53 en 4 iniciativas
| SPEC-WRITE-0001 | spec-native/specs/write-engine/SPEC.md | closed | 2026-07-30 |
| SPEC-CONTAINERS-0001 | spec-native/specs/containers/SPEC.md | closed | 2026-07-30 |
| SPEC-TOOL-0001 | spec-native/specs/tooling/SPEC.md | closed | 2026-07-30 |
| SPEC-AUTH-0001 | spec-native/specs/authentication/SPEC.md | closed | 2026-07-30 |

## Validación de capacidad

| Tarea | Implementación | Evidencia | Decisión |
|---|---|---|---|
| `TASK-VAL-0037` | `benchmarks/`, `just benchmark-db` | `spec-native/evidence/DB-COMPARISON-20260811.md` | `DEC-0063`, `DEC-0064` |
| `TASK-VAL-0034` | writer/projector progress metrics, `k8s/alerts.yaml` | promtool passed; controlled alert activation pending | pending |
| `TASK-VAL-0025` | writer queue/SQLite/ACK/cache histograms | `spec-native/evidence/PRODUCTION-HARDENING-20260811.md` | done |
| `TASK-VAL-0033` | `crash_puback_window.sh`, atomic PUBACK barrier | `spec-native/evidence/PRODUCTION-HARDENING-20260811.md` | done |
| `TASK-VAL-0035` | opt-in progress watchdog and MQTT diagnostics | pending stall reproduction | pending |
| `TASK-VAL-0036` | Pattern R reverse index + reconciler rebuild | `spec-native/evidence/PRODUCTION-HARDENING-20260811.md` | `DEC-0065` |

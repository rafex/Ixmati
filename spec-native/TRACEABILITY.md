# TRACEABILITY.md

Vínculos entre artefactos del proyecto.

## Especificaciones

| Spec | Estado | Owner | Tareas | Decisiones |
|---|---|---|---|---|
| `SPEC-AUTH-0001` | `active` | team-auth | `TASK-AUTH-0001`..`0003` | — |
| `SPEC-WRITE-0001` | `active` | team-core | 21 todo + 3 cancelled | `DEC-0001`..`DEC-0020` |
| `SPEC-TOOL-0001` | `active` | team-core | 13 done + 1 todo | `DEC-0021`..`DEC-0027` |
| `SPEC-CONTAINERS-0001` | `active` | team-core | 11 done + 1 todo | `DEC-0028`..`DEC-0033` |
| `SPEC-PROTOBUF-0001` | `active` | team-core | `TASK-PROTO-0001`..`0006` | `DEC-0069` |

## Total

- **Decisiones**: el histórico está en `DECISIONS.md`; la última es `DEC-0083`
- **Tareas**: el histórico está en las carpetas de `tasks/`; se agregó
  `protobuf-api` con seis tareas
| SPEC-WRITE-0001 | spec-native/specs/write-engine/SPEC.md | closed | 2026-07-30 |
| SPEC-CONTAINERS-0001 | spec-native/specs/containers/SPEC.md | closed | 2026-07-30 |
| SPEC-TOOL-0001 | spec-native/specs/tooling/SPEC.md | closed | 2026-07-30 |
| SPEC-AUTH-0001 | spec-native/specs/authentication/SPEC.md | closed | 2026-07-30 |

## Validación de capacidad

| Tarea | Implementación | Evidencia | Decisión |
|---|---|---|---|
| `TASK-VAL-0037` | `benchmarks/`, `just benchmark-db` | `spec-native/evidence/DB-COMPARISON-20260811.md` | `DEC-0063`, `DEC-0064` |
| `TASK-VAL-0034` | writer/projector progress metrics, `k8s/alerts.yaml` | `k8s/alerts.test.yaml`, promtool: 13/13 | done |
| `TASK-VAL-0025` | writer queue/SQLite/ACK/cache histograms | `spec-native/evidence/PRODUCTION-HARDENING-20260811.md` | done |
| `TASK-VAL-0033` | `crash_puback_window.sh`, atomic PUBACK barrier | `spec-native/evidence/PRODUCTION-HARDENING-20260811.md` | done |
| `TASK-VAL-0035` | opt-in progress watchdog, MQTT diagnostics, and `_idempotency` covering index | `helpers/shell/mqtt_stall_probe.sh`, `crates/ixmati-writer/src/dedup.rs`, `spec-native/evidence/MQTT-STALL-DIAGNOSTIC-20260811.md` | `DEC-0067` |
| `TASK-VAL-0036` | Pattern R reverse index + reconciler rebuild | `spec-native/evidence/PRODUCTION-HARDENING-20260811.md` | `DEC-0065` |
| `TASK-VAL-0025` | post-index durable writer baseline and rate-controlled staircase | `spec-native/evidence/LOAD-POST-INDEX-20260811.md` | `DEC-0067` |
| `TASK-PROD-0001` | `benchmarks/ixmati-soak.jmx`, `benchmarks/soak_capacity.sh`, `ixmati-protocol-bench` | `spec-native/evidence/PRODUCTION-PROFILE-10S-SHA-6C38EB8-20260813.md`; smoke post-hardening `PRODUCTION-PROFILE-10S-SHA-359CF89-SMOKE-20260813.md` | `DEC-0077` |
| `TASK-CAP-0001` | `benchmarks/ixmati-soak.jmx`, `benchmarks/soak_capacity.sh`, bounded `rate_load.py`, `crates/ixmati-writer/src/event_publisher.rs` ACK-driven | `spec-native/evidence/PRODUCTION-STATUS-20260813.md`; `DEC-0083`, `DEC-0084`; 150/s: 45,000 comandos, 93.92% HTTP 200, p99 2.077s, outbox 0; 200/s inconcluso; falta soak de una hora | pending |
| `TASK-STORE-0001`..`0006` | `crates/ixmati-store-migrate`, tombstones, migration runbook | `spec-native/evidence/STORE-MIGRATION-E2E-20260813.md`; cutover remoto pendiente | `DEC-0068` |
| `TASK-WRITE-0014` | `config/litestream.yml`, `containers/litestream/`, `helpers/python/installer.py`, `helpers/shell/litestream_restore.sh`, `helpers/shell/test_litestream_local.sh`, `helpers/shell/test_litestream_s3.sh` | `PRODUCTION-BACKUP-LOCAL-20260813.md` + `PRODUCTION-BACKUP-LOCAL-FILE-20260813.md` + `PRODUCTION-BACKUP-S3-COMPAT-20260813.md`; bucket remoto real, segundo destino y RPO/RTO pendientes | `DEC-0078` |
| `TASK-PROTO-0001` | `proto/ixmati/v1/`, `build.rs`, `make proto` | `cargo check -p ixmati-api`, `make proto` | `DEC-0069` |
| `TASK-PROTO-0002`..`0003` | `crates/ixmati-api/src/grpc.rs`, REST dispatch and generated tonic client | `spec-native/evidence/PROTOBUF-E2E-20260812.md` | `DEC-0069` |
| `TASK-PROTO-0004` | `EventService.SubscribeEvents` replay/live, buffer acotado y cursor de reanudación | `spec-native/evidence/PROTOBUF-E2E-20260812.md`; tests locales de cliente lento/backpressure; integración remota de retención/reconexión pendiente | `DEC-0069`, `DEC-0080` |
| `TASK-PROTO-0005` | deployment, OpenAPI, README, docs and SpecNative | `spec-native/evidence/PROTOBUF-E2E-20260812.md`; config/distribution gates pass | `DEC-0069` |
| `TASK-PROTO-0006` | `benchmarks/protocol_benchmark.sh`, `ixmati-protocol-bench` | `spec-native/evidence/PROTOBUF-BENCH-20260812.md` | `DEC-0069` |
| `TASK-PROD-0002` | REST body limits, gRPC message/stream limits, slow-stream terminal cursor | `spec-native/evidence/API-HARDENING-20260813.md`; gates locales y distribución amd64 | `DEC-0080` |
| `TASK-PROD-0003` | API-key scopes en REST/gRPC, protección de lecturas/estados y persistencia del instalador | `spec-native/evidence/API-SCOPES-20260813.md`; `cargo test -p ixmati-api` (53/53) | `DEC-0081` |

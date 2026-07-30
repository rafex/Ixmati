# Ixmati

Motor de escritura serializada para SQLite con soporte multi-backend.

## Qué es

Ixmati permite que múltiples backends o pods escriban en una misma instancia de SQLite sin contención. Las escrituras pasan por un canal de ingesta desacoplado (API REST/gRPC o MQTT) y son procesadas secuencialmente por un único writer. Las lecturas se sirven desde una cache rápida con fallback a SQLite. Litestream replica el WAL a destinos remotos para disaster recovery.

## Stack

Rust (tokio, axum, tonic, rusqlite, rumqttc) · Mosquitto (persistence + QoS 1) · SQLite (WAL + synchronous=NORMAL) · FlashDB (cache) · Litestream (backup continuo).

## Navegación

| Documento | Propósito |
|---|---|
| [`spec-native/PRODUCT.md`](spec-native/PRODUCT.md) | Problema, usuarios, objetivos, métricas |
| [`spec-native/ARCHITECTURE.md`](spec-native/ARCHITECTURE.md) | Opciones de arquitectura, módulos, restricciones |
| [`spec-native/STACK.md`](spec-native/STACK.md) | Tecnologías, versiones, notas de riesgo |
| [`spec-native/CONVENTIONS.md`](spec-native/CONVENTIONS.md) | Código, naming, envelope, errores, logging |
| [`spec-native/COMMANDS.md`](spec-native/COMMANDS.md) | Build, test, docker, resync, restore |
| [`spec-native/DECISIONS.md`](spec-native/DECISIONS.md) | Decisiones de arquitectura (ADRs) |
| [`spec-native/ROADMAP.md`](spec-native/ROADMAP.md) | Fases y prioridades |
| [`spec-native/TRACEABILITY.md`](spec-native/TRACEABILITY.md) | Vínculos entre artefactos |
| [`spec-native/SESSION.md`](spec-native/SESSION.md) | Estado activo de trabajo |
| [`TODO.md`](TODO.md) | Tablero de tareas |

### Especificación y tareas

- **Spec**: [`spec-native/specs/write-engine/SPEC.md`](spec-native/specs/write-engine/SPEC.md) — SPEC-WRITE-0001
- **Tareas**: [`spec-native/tasks/write-engine/TASKS.md`](spec-native/tasks/write-engine/TASKS.md) — TASK-WRITE-0001 a TASK-WRITE-0016

## Licencia

MIT. Ver [`LICENSE`](LICENSE).

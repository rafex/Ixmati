# Ixmati

Motor de escritura serializada para SQLite con soporte multi-backend. **Estado:
beta técnica**, validada en Debian amd64 con carga controlada.

## Qué es

Ixmati permite que múltiples backends o pods escriban en una misma instancia de SQLite sin contención. Las escrituras pasan por un canal de ingesta desacoplado (API REST/gRPC o MQTT) y son procesadas secuencialmente por un único writer. Las lecturas se sirven desde una cache rápida con fallback a SQLite. Litestream replica el WAL a destinos remotos para disaster recovery.

## Uso rápido

Para probar el stack completo localmente:

```bash
cd examples/quickstart
docker compose up -d
./e2e-test.sh
```

La [guía de uso](docs/src/usage.md) cubre Docker, instalación nativa con
systemd, configuración, API REST, métricas, backup y recuperación.

Una escritura durable se envía así:

```bash
curl -X POST http://localhost:8080/write \
  -H "Authorization: ApiKey $IXMATI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"op":"upsert","store":"pedidos","entity":"pedido","key":"ped_1","version":1,"idempotency_key":"pedido-1-v1","ack_mode":"committed","payload":{"total":1500}}'
```

`accepted` es un alias compatible de `committed`: `200` significa que el
commit SQLite quedó confirmado, mientras que `202` significa que la solicitud
quedó pendiente y debe consultarse con `GET /writes/{store}/{idempotency_key}`.

## Capacidad y límites conocidos

- El throttle productivo predeterminado es de 40 escrituras por segundo.
- La validación rate-controlled en Debian amd64 sostuvo 40–120 solicitudes/s
  sin `202` ni crecimiento de `consumer_queue_depth`.
- 150 solicitudes/s produjo la primera señal de saturación; no se presenta
  como capacidad sostenible.
- La entrega de eventos es at-least-once: un crash en la ventana de PUBACK
  puede producir duplicados, pero no debe perder eventos confirmados.
- Cada store tiene un writer y un archivo SQLite; sharding interno, clustering
  de Mosquitto y failover transparente siguen fuera del alcance de esta beta.

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
| [`docs/src/usage.md`](docs/src/usage.md) | Guía de uso para desarrolladores y operadores |

### Especificación y tareas

- **Spec**: [`spec-native/specs/write-engine/SPEC.md`](spec-native/specs/write-engine/SPEC.md) — SPEC-WRITE-0001
- **Tareas**: [`spec-native/tasks/write-engine/TASKS.md`](spec-native/tasks/write-engine/TASKS.md) — TASK-WRITE-0001 a TASK-WRITE-0016

## Licencia

MIT. Ver [`LICENSE`](LICENSE).

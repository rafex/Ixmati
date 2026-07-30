+++
[session]
state = "in_progress"
agent = "opencode"
initiative = "write-engine"
task = "documentation"
intent = "Segunda ronda de documentacion: arquitectura unificada con Store, outbox transaccional, bus de eventos, proyecciones y cache dual. 20 decisiones, 25 tareas."
last_updated = "2026-07-29"
+++

# Active Session

## Current state

Documentacion base del proyecto Ixmati actualizada tras la revision de la propuesta "SQLite por dominio" y "FlashDB como capa de joins".

Cambios clave respecto a la ronda anterior:
- **Store** como primitivo del motor (no "dominio"). `stores=1` es el caso base sin overhead.
- **Arquitectura unificada**: Opcion A (Mosquitto) confirmada. DEC-0010 y DEC-0011 cerradas por diseno.
- **Transactional outbox**: los eventos se escriben en `_outbox` dentro de la misma transaccion que los datos. 0 eventos perdidos por diseno.
- **Bus de eventos separado**: `ixmati/cmd/...` vs `ixmati/evt/...` con semantica y retencion distintas.
- **Proyecciones opt-in**: cache-aside por defecto; read models proyectados se declaran en config (patron R y M).
- **Reconciler fan-in**: reproyeccion offline desde N stores (reemplaza el resync mono-store).
- **Cache dual**: namespaces `c:` (cache-aside) y `p:` (proyecciones) en FlashDB, purgables por separado.
- **Sin sagas en el motor**: el motor provee primitivas (outbox, idempotencia, eventos); la aplicacion coordina.
- **ATTACH read-only** como escape hatch para reporting cross-store.

20 decisiones: 16 accepted, 2 superseded, 1 cancelled, 1 replaced.
25 tareas: 21 todo, 2 cancelled, 2 sin estado (implied by superseded).
`TASK-WRITE-0002` cancelada (spike A/B).
`TASK-WRITE-0013` cancelada (resync mono-store reemplazado por reconciler).

## Next steps

1. Ejecutar `TASK-WRITE-0001` — spike de viabilidad de FlashDB via FFI en Rust (severidad alta: alojara read models, no solo cache).
2. Ejecutar `TASK-WRITE-0003` — definir contratos de envelope (comando + evento), topics (cmd + evt) y .proto.
3. Ejecutar `TASK-WRITE-0004` — definir OpenAPI.
4. Proceder con `TASK-WRITE-0005` (ixmati-core con StoreConfig, ambos envelopes) y `TASK-WRITE-0017` (store registry).
5. Implementar writer + outbox (`0006` + `0018`) y tests de crash (`0007`).

## Context for next agent

- Las decisiones de arquitectura (A/B, lectura) estan **cerradas**. No hay ambiguedad. Ver DEC-0010 y DEC-0011.
- FlashDB es el unico riesgo abierto (DEC-0009). Si el spike falla, la migracion a sled/redb/lmdb-rs es transparente gracias al trait `CacheBackend`.
- El campo `store` es **obligatorio** en todo comando. El motor no funciona sin el.
- Los eventos (`ixmati/evt/...`) y comandos (`ixmati/cmd/...`) estan en topics separados. No mezclarlos.
- El outbox es transaccional (misma `BEGIN IMMEDIATE` que los datos). El publicador es una task interna del writer.
- `stores=1` debe funcionar sin bus de eventos, sin outbox, sin proyectores, sin reconciler. El overhead de eventos solo se activa con `stores > 1` o proyecciones declaradas.
- Stores son inmutables tras creacion. Renombrar un store es una migracion (nuevo archivo + nuevo Litestream + redireccion de topics).
- Las proyecciones son opt-in. Sin proyecciones declaradas, el projector y el reconciler no hacen nada.
- `specs/authentication/` y `tasks/authentication/` son ejemplos del framework SpecNative, NO son parte del proyecto.
- Formato de metadata: `+++` para SESSION.md, ` ```toml ` para specs y tasks.

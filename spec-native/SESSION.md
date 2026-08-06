+++
[session]
state = "in_progress"
agent = "opencode"
initiative = "projector-validation"
task = "Fase-1-cache-refactor"
intent = "Activar proyecciones reales, reconciler y multi-store con Redb funcional via cache-server dedicado"
last_updated = "2026-08-05T23:00:00Z"
+++

# Active Session

## Current state

Iniciativa `projector-validation` en progreso. Fases 0, 1 y 4 completadas (compilando). Fases 2, 3, 5, 6, 7 pendientes.

### Completado
- **Fase 0**: EventPublisher emite `EventEnvelope` completo en `ix/evt/...`
- **Fase 1**: Protocolo socket extendido (GET/SET/DEL/DEL_PREFIX/FLUSH) movido a `ixmati-cache`
  - `CacheServer` + `CacheClient` en crate compartido
  - Binario `ixmati-cache-server` (dueño único Redb) + Containerfile
  - Keyspace unificado `p:` en pattern_r/pattern_m
  - Writer usa `CacheClient` (socket) para cache_sync, ya no abre Redb directo
  - API simplificada: modo socket como default
- **Fase 4**: API `?projection=&key=` implementada (lectura de `p:` via socket)

### Pendiente (por fases)
- **Fase 2** — Projector real: suscribirse a `ix/evt/#`, dedup `event_id`, escribir `p:*` vía socket client
- **Fase 3** — Reconciler real: fan-in con `ReadOnlyConnection` + ATTACH, escribir `p:*` vía socket
- **Fase 5** — Multi-store compose con `cache-server` (pedidos/usuarios/inventario)
- **Fase 6** — All-in-one actualizado con `ixmati-cache-server` en supervisord.conf
- **Fase 7** — Validación e2e (e-commerce) + DEC

## Archivos modificados
- `crates/ixmati-writer/src/outbox.rs` — payload = EventEnvelope completo
- `crates/ixmati-writer/src/cache_server.rs` — re-export desde ixmati-cache
- `crates/ixmati-writer/src/cache_sync.rs` — usa CacheClient (socket)
- `crates/ixmati-writer/src/main.rs` — conecta a cache-server por socket
- `crates/ixmati-api/src/cache_client.rs` — re-export desde ixmati-cache
- `crates/ixmati-api/src/rest.rs` — proyecciones + writeback socket
- `crates/ixmati-api/src/lib.rs` — modo socket default
- `crates/ixmati-cache/src/cache_server.rs` — NUEVO: protocolo extendido
- `crates/ixmati-cache/src/cache_client.rs` — NUEVO: API completa
- `crates/ixmati-cache/src/lib.rs` — exports CacheServer, CacheClient
- `crates/ixmati-cache-server/src/main.rs` — NUEVO: binario
- `crates/ixmati-cache-server/Cargo.toml` — NUEVO
- `crates/ixmati-projector/src/pattern_r.rs` — keyspace `p:`
- `crates/ixmati-projector/src/pattern_m.rs` — keyspace `p:`
- `containers/cache-server/Containerfile` — NUEVO
- `Cargo.toml` — workspace member cache-server

## Next steps (mañana)
1. `cargo check` para verificar compilación limpia
2. Fase 2: `crates/ixmati-projector/src/main.rs` — MQTT consumer + process_event + socket SET
3. Fase 3: `crates/ixmati-reconciler/src/main.rs` — fan-in stores + socket SET
4. Fase 5: actualizar `containers/compose/multi-store.yaml` con cache-server
5. Fase 6: actualizar `containers/allinone/supervisord.conf` con cache-server
6. Fase 7: validación e2e con caso e-commerce

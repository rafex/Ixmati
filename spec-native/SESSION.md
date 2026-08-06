+++
[session]
state = "idle"
agent = "opencode"
initiative = "projector-validation"
task = "done"
intent = "Cerrar iniciativa: todas las fases completadas, bugs resueltos, 117 unit + 4 e2e tests pasan"
last_updated = "2026-08-06T17:57:37Z"
+++

# Active Session

## Current state

Cerrar iniciativa: todas las fases completadas, bugs resueltos, 117 unit + 4 e2e tests pasan

## Next steps

Iniciativa cerrada. El próximo agente puede:
1. Tomar TASK-SMOKE-0003/0004 del TODO.md
2. Iniciar nueva iniciativa según ROADMAP.md
3. Ejecutar tests e2e con: E2E_EXTERNAL=1 pytest tests/smoke/test_ecommerce.py -m e2e -v

## Context for next agent

Iniciativa projector-validation COMPLETADA.

Bugs encontrados y resueltos:
1. cache-server idle timeout 30s → eliminado (cerraba conexión antes del primer evento)
2. process_r_async cross-store lookup usaba event.entity/key → fix con resolve_lookup_key + infer_entity_from_store
3. CacheClient::read_simple_response timeout 20ms → 5s + logging
4. Cache-server EXPOSE 0 inválido → eliminado
5. Cache-server USER ixmati → 1000:1000 + chmod 777
6. MQTT_BROKER unificado en compose
7. CACHE_READ_MODE=socket + IXMATI_API_KEYS agregados
8. Paths relativos en compose corregidos (../../config/...)
9. Mosquitto healthcheck arreglado (mosquitto_pub en vez de mosquitto_sub con $$)
10. Projector Containerfile copia projections.toml embebido (podman macOS no soporta bind mounts)

Tests: 117 unitarios + 4 e2e (Pattern M, Pattern R, idempotencia, concurrencia) = todos pasan.

Archivos clave modificados:
- crates/ixmati-cache/src/cache_server.rs (timeout)
- crates/ixmati-cache/src/cache_client.rs (timeout + logging)
- crates/ixmati-cache/src/redb_backend.rs (tests key composition)
- crates/ixmati-projector/src/main.rs (MQTT consumer + debug logging)
- crates/ixmati-projector/src/pattern_r.rs (resolve_lookup_key + infer_entity_from_store + tests)
- crates/ixmati-projector/src/pattern_m.rs (debug logging + tests)
- crates/ixmati-projector/src/lib.rs (process_event_async)
- crates/ixmati-core/src/projection.rs (TOML deserialization test)
- crates/ixmati-reconciler/src/lib.rs + main.rs (fan-in + CacheClient)
- containers/base/Containerfile (ixmati-cache-server binary)
- containers/cache-server/Containerfile (USER 1000:1000, chmod 777)
- containers/projector/Containerfile (COPY projections.toml)
- containers/compose/multi-store.yaml (cache-server service + fixes)
- containers/compose/single-store.yaml (cache-server + fixes)
- containers/compose/smoke.yaml (cache-server + fixes)
- containers/allinone/supervisord.conf (cache-server program)
- containers/allinone/Containerfile (cache-server binary)
- config/projections.toml (proyecciones activadas)
- helpers/make/containers.mk (cache-server en SERVICES)
- helpers/python/mqtt_harness.py (http_read_projection, Python 3.9 compat)
- tests/smoke/conftest.py (compose_up_multi fixtures, Python 3.9 compat)
- tests/smoke/test_ecommerce.py (4 tests e2e)

DEC-0037, DEC-0038, DEC-0039 registradas.

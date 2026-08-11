+++
[session]
state = "waiting_handoff"
agent = "codex"
initiative = "validation-durability-hardening"
task = "TASK-VAL-0037"
intent = "Ejecutar comparativa reproducible SQLite directo, Ixmati y PostgreSQL"
last_updated = "2026-08-11T06:40:00Z"
+++

# Active Session

## Current state

Validación de cache y proyecciones ejecutada en Debian amd64. El compose
multi-store fue corregido para montar cada SQLite como directorio de store;
cache-aside, Pattern M, Pattern R inicial, idempotencia y concurrencia pasan.
Se agregó reconexión del CacheClient después de reiniciar cache-server y
resolución de hostnames en `/health`. Pattern R no refresca automáticamente
referencias existentes cuando cambia el store relacionado; queda como
TASK-VAL-0036. El build/check de mdBook sigue pendiente porque `mdbook` no está
instalado en el entorno local.

La suite de comparativa de capacidad está implementada en `benchmarks/` y
pendiente de ejecución completa en Debian. Separará SQLite directo, PostgreSQL
18 e Ixmati, conservará tres repeticiones por escenario y distinguirá las
referencias oficiales de PostgreSQL de los resultados medidos.

## Next steps

La lectura cacheada se validó a 100–500/s sin errores; a 1000/s no hubo errores
pero apareció saturación del generador y p99≈710ms en cache-aside. Después deben
priorizarse Pattern R mutable, el crash determinista entre PUBACK y
`published_at`, las alertas operativas y el atasco MQTT bajo sobrecarga extrema.

## Context for next agent

La iniciativa native-installer-hardening anterior está COMPLETADA.

## Current handoff — validation-durability-hardening

Implementado y validado localmente:
- ACK MQTT del consumidor después del commit SQLite; la cola en memoria ya no
  es una frontera de éxito.
- API `200` sólo después de `_idempotency`; `accepted` es alias durable y
  `SQLITE_PATHS` permite confirmar por store en multi-store.
- Outbox marcado sólo después de PUBACK; el contrato de eventos queda
  explícitamente at-least-once.
- Métricas reales de cola/ACK/cache/último commit y escalera que prefiere wrk2
  y no convierte una serie ausente en cero.
- `cargo test --workspace`, clippy estricto, `just validate-config`, `bash -n`
  y `git diff --check` pasan.
- Se corrigió el contexto de COPY del Containerfile de Litestream y se
  actualizaron los compose para rutas SQLite single/multi-store.

Resultados del ciclo actual:
1. El defecto era el flush temporal: con tráfico continuo `recv_timeout` no
   expiraba y el batcher esperaba llenar el batch. `push()` ahora aplica el
   límite temporal y tiene una regresión específica.
2. El árbol `9d96b0b` sostuvo exactamente 40/60/80/100/120 solicitudes/s en
   Debian con `200`, sin `202` ni saturación del generador; p99 fue
   147/205/281/375/252ms respectivamente y la cola MQTT quedó en 0.
3. A 150/s aparecieron `202 PENDING`, p99≈2.13s y cola MQTT=100; es el primer
   escalón de saturación y no debe presentarse como capacidad sostenible.
4. El crash/restart anterior validó 30/30 escrituras y 30/30 eventos sin
   pérdida observada; la ventana PUBACK→marca permanece pendiente de prueba
   forzada.

Motivo: el usuario preguntó si Ixmati ya tenía un instalador "listo para usar"
estilo PostgreSQL/MongoDB para Linux Debian. El instalador nativo existía
(`scripts/install.sh` + `helpers/python/installer.py` + `make dist`) pero
tenía un bug de arquitectura real: predataba a DEC-0037 y nunca se actualizó
cuando `ixmati-cache-server` pasó a ser el dueño único de Redb. Una
instalación limpia aceptaba escrituras pero no servía cache-aside ni
proyecciones.

Bugs encontrados y resueltos (ver DEC-0040 para detalle completo):
1. `ixmati-cache-server` no estaba en `BINARIES`/`SYSTEMD_UNITS` del instalador, no tenía unidad systemd, nunca se arrancaba
2. `ixmati-projector` nunca se arrancaba en `start_services()`
3. `ixmati-projector`/`ixmati-reconciler` leían `IXMATI_MQTT_BROKER`/`IXMATI_CACHE_SOCKET` en vez de `MQTT_BROKER`/`CACHE_SOCKET_PATH` — unificado en el código Rust
4. `install_config()` sobrescribía `stores.toml`/`projections.toml` en cada reinstalación, pisando ediciones del usuario
5. `scripts/install.sh` invocaba `python3` sin verificarlo — Debian mínimo no lo trae preinstalado
6. `configure_mosquitto()` copiaba como fragmento `conf.d/ixmati.conf`, pero el paquete Debian de Mosquitto ya define `persistence_location` en su config por defecto → Mosquitto rechazaba el arranque (`exit-code 3/NOTIMPLEMENTED`, "Duplicate persistence_location value")
7. `install_binaries()` fallaba con `Text file busy` al reinstalar sobre binarios en ejecución

Se agregó además: modo `--uninstall`/`--uninstall --purge`, `verify_health()`
con curl real, y validación end-to-end automatizada en
`debian:trixie-slim` con systemd como PID 1 dentro de Podman `--privileged`.

Tests: cargo test --workspace --lib → 0 fallos (105 tests). Validación
completa en contenedor Debian: instalación limpia (5 servicios activos,
health 200, write/read via cache-server), reinstalación idempotente, y
desinstalación con purga — las 3 pasadas exitosas.

Archivos clave modificados:
- crates/ixmati-projector/src/main.rs (env vars unificadas)
- crates/ixmati-reconciler/src/main.rs (env vars unificadas)
- containers/compose/multi-store.yaml (env vars unificadas)
- systemd/ixmati-cache-server.service (nuevo)
- systemd/ixmati-api.service, ixmati-writer@.service, ixmati-projector.service (deps + env vars)
- helpers/python/installer.py (cache-server, projector, uninstall/purge, idempotencia, verify_health)
- scripts/install.sh (bootstrap de python3)
- config/mosquitto/mosquitto.conf (marcador de idempotencia)
- helpers/make/artifacts.mk, dist-validate.mk (cache-server incluido)
- containers/installer-test/Containerfile (nuevo, Debian + systemd)
- helpers/shell/test_installer_debian.sh (nuevo)
- helpers/just/installer.just (nuevo, `just installer-test`)
- TODO.md (sección Native Installer Hardening, TASK-INST-0001..0006)

DEC-0040 registrada.

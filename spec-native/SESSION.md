+++
[session]
state = "waiting_handoff"
agent = "codex"
initiative = "validation-durability-hardening"
task = "TASK-VAL-0027..0029"
intent = "Endurecer durabilidad de ACKs, outbox y contrato HTTP; dejar gates verdes"
last_updated = "2026-08-10T23:59:00Z"
+++

# Active Session

## Current state

Cerrar iniciativa: instalador nativo funcionalmente completo, validado en Debian real

## Next steps

Iniciativa cerrada. El próximo agente puede:
1. Ejecutar `just installer-test` para revalidar tras cualquier cambio en installer.py/systemd/*
2. Considerar reiniciar servicios automáticamente en upgrades de versión (ver DEC-0040, consecuencia pendiente)
3. Validar el tarball con binarios `linux-amd64` reales (host remoto amd64, ver DEC-0033) — esta iniciativa validó con binarios `linux/arm64` locales
4. Continuar con `Fase 5 — Observabilidad, consistencia y hardening` del ROADMAP.md

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

Pendiente para el siguiente ciclo:
1. Ejecutar la escalera en Debian nativo con wrk2/vegeta y registrar números
   nuevos; los resultados de DEC-0058 no deben reutilizarse para afirmar un
   punto de quiebre.
2. Ejecutar crash/restart durante ingestión y entre PUBACK/mark del outbox.
3. Continuar TASK-VAL-0025: aislar el tiempo del 61.3% no explicado.

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

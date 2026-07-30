+++
[session]
state = "in_progress"
agent = "opencode"
initiative = "write-engine"
task = "documentation"
intent = "Crear el contexto base del proyecto: PRODUCT, ARCHITECTURE, STACK, CONVENTIONS, COMMANDS, DECISIONS, ROADMAP, SPEC, TASKS, TRACEABILITY, TODO"
last_updated = "2026-07-29"
+++

# Active Session

## Current state

Documentación base del proyecto Ixmati (motor de escritura serializada para SQLite) completada.
- `PRODUCT.md`: problema, usuarios, objetivos, métricas, no-objetivos, valor diferencial.
- `ARCHITECTURE.md`: opciones A y B documentadas como abiertas, con tabla comparativa y criterios de decisión.
- `STACK.md`: Rust, Mosquitto, SQLite, FlashDB (con nota de riesgo), Litestream.
- `CONVENTIONS.md`: layout Cargo, naming, topics MQTT, envelope, errores, logging, tests.
- `COMMANDS.md`: build, test, clippy, docker, resync, restore, health checks.
- `DECISIONS.md`: DEC-0001 a DEC-0009 como `accepted`, DEC-0010 y DEC-0011 como `proposed`.
- `ROADMAP.md`: fases F0 a F5 con tareas mapeadas.
- `specs/write-engine/SPEC.md`: SPEC-WRITE-0001, state `active`.
- `tasks/write-engine/TASKS.md`: TASK-WRITE-0001 a TASK-WRITE-0016, todas en `todo`.
- `TRACEABILITY.md`: vínculos completos.
- `TODO.md`: creado con items de documentación.
- `README.md` raíz: descripción + índice de navegación.

## Next steps

1. Ejecutar `TASK-WRITE-0001` — spike de viabilidad de FlashDB vía FFI en Rust.
2. Ejecutar `TASK-WRITE-0002` — spike comparativo Opción A vs Opción B.
3. Cerrar DEC-0010 y DEC-0011 con la evidencia de los spikes.
4. Proceder con `TASK-WRITE-0003` (contratos .proto y envelope).

## Context for next agent

- Las decisiones DEC-0010 y DEC-0011 están **abiertas** (`proposed`). No se puede empezar a codificar `ixmati-writer` ni `ixmati-api` hasta cerrarlas.
- La arquitectura tiene **dos opciones en evaluación**. La Opción A (Mosquitto buffer) es la más conservadora y probablemente la que prevalecerá. La Opción B (FlashDB buffer) tiene problemas de durabilidad.
- FlashDB es una librería C para microcontroladores. El spike debe determinar si el FFI es viable en Linux x86_64. Si no lo es, sled es la alternativa más probable.
- `specs/authentication/` y `tasks/authentication/` son ejemplos del framework SpecNative, NO son parte del proyecto. Se dejaron como referencia de formato.
- El semestre de escritura es **dual** (async/sync, seleccionable por request). Esto afecta el diseño de la API y del writer desde el día 1.
- El formato de metadata es toml (`+++` para SESSION.md, ` ```toml ` para specs y tasks).

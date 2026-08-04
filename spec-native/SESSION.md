+++
[session]
state = "in_progress"
agent = "unknown"
initiative = "smoke-tests"
task = "TASK-SMOKE-0001"
intent = "Completar ejecución de smoke tests (standalone + E2E). Se reconstruyó entorno desde cero (repo + imágenes en remoto), se corrigieron 5 bugs (MQTT EventLoop descartado, API sin SQLITE_PATH, conftest compose_up para SMOKE_HOST, run_podman SSH wrapper, sqlite3 faltante en writer), y se ajustaron tests para infraestructura remota."
last_updated = "2026-08-04T17:07:42Z"
+++

# Active Session

## Current state

Completar ejecución de smoke tests (standalone + E2E). Se reconstruyó entorno desde cero (repo + imágenes en remoto), se corrigieron 5 bugs (MQTT EventLoop descartado, API sin SQLITE_PATH, conftest compose_up para SMOKE_HOST, run_podman SSH wrapper, sqlite3 faltante en writer), y se ajustaron tests para infraestructura remota.

## Next steps

1. Revisar los cambios en git diff antes de commit
2. Actualizar spec-native/DECISIONS.md con los bugs raíz encontrados
3. Marcar TASK-SMOKE-0001 como done
4. Cerrar iniciativa smoke-tests

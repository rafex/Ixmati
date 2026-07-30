# Tasks

Indice del sistema de ejecucion del proyecto.

## Objetivo

Traducir specs en unidades ejecutables, con estado observable y
criterio de cierre verificable.

## Reglas

- Toda carpeta de iniciativa en `tasks/` debe corresponder a una spec.
- Toda tarea debe declarar: ID, titulo, estado, owner y criterio de
  cierre.
- No usar `tasks/` como lista de ideas. Solo trabajo derivado de una
  spec vigente.
- Si una tarea se bloquea, registrar bloqueo y dependencia.

## Estructura sugerida

```text
spec-native/tasks/
  README.md
  TASKS.template.md
  authentication/
    README.md
    TASKS.md
```

## Flujo

1. Leer la spec asociada en `../specs/<iniciativa>/`.
2. Descomponer en tareas pequenas y validables.
3. Ejecutar segun prioridad y dependencias.
4. Actualizar estado real durante la ejecucion (via MCP: `update_task`).
5. Reflejar cierre y evidencia en `../TRACEABILITY.md`.

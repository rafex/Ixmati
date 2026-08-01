Captura una solicitud del desarrollador en el backlog de SpecNative sin editar
un tablero generado.

Solicitud recibida: `$ARGUMENTS`

## Proceso

1. Identifica el título y la descripción de `$ARGUMENTS`. Si falta una de las
   dos, pide la información faltante.
2. Llama `list_specs()` y `board()` vía MCP para localizar una iniciativa
   existente y evitar duplicados.
3. Si el desarrollador indicó una iniciativa con spec y la solicitud tiene
   criterio de cierre y al menos una validación, llama:

   ```text
   capture_backlog_item(title, description, initiative, priority, owner,
                        close_criteria, validation, dependencies)
   ```

4. Si no existe una spec relacionada o faltan criterio de cierre o validación,
   llama `capture_backlog_item(title, description, priority=...)` sin
   `initiative`. Esto guarda una idea `triaged` en `spec-native/intake/IDEAS.md`.
5. Explica el resultado con precisión:
   - una tarea canónica aparecerá en `board()` como `ready` o `waiting`;
   - una idea intake no es ejecutable hasta que se promueva a una spec.

No crees ni edites una tarjeta Markdown o Mermaid directamente. No inventes
criterios de cierre, validaciones ni dependencias cuando la solicitud no los
aporte.

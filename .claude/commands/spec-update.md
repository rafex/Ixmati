Revisa el estado de la documentación SpecNative, detecta qué está incompleto o desactualizado, y guía al desarrollador a refinarlo iterativamente.

## Proceso

1. Llama `health_check()` via MCP — muestra el reporte completo al desarrollador.

2. Llama `suggest_next()` via MCP — muestra las 3 acciones más importantes.

3. Pregunta al desarrollador:
   > "¿Qué quieres hacer hoy?"
   > a) Llenar los vacíos detectados
   > b) Actualizar un documento específico (¿cuál?)
   > c) Iniciar una nueva spec para una iniciativa
   > d) Refinar el ROADMAP con nuevas prioridades

4. Según la elección:

   **a) Llenar vacíos:**
   - Para cada doc con gaps en el health_check, haz las preguntas específicas faltantes.
   - Usa `refine_document(doc, what_changed, new_content)` para actualizar.

   **b) Documento específico:**
   - Lee el doc con `read_context(doc)`.
   - Muéstraselo al desarrollador y pregunta: ¿qué cambió desde que se escribió esto?
   - Actualiza con `refine_document()`.

   **c) Nueva spec:**
   - Usa el prompt MCP `start_initiative(name, problem)` como guía.
   - Crea `spec-native/specs/<iniciativa>/SPEC.md` con el contenido acordado.

   **d) Actualizar ROADMAP:**
   - Lee el ROADMAP con `read_context('roadmap')`.
   - Pregunta: ¿qué cambió en las prioridades? ¿qué se completó? ¿qué llega nuevo?
   - Actualiza con `refine_document('roadmap', ...)`.

5. Al terminar, llama `health_check()` de nuevo para confirmar que los gaps se redujeron.

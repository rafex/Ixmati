Genera un traspaso estructurado para que el siguiente agente — Claude Code, Codex, OpenCode u otro — pueda continuar exactamente donde estás ahora.

## Proceso

1. Llama `status()` via MCP para ver el estado actual de specs y tareas.

2. Llama `resume()` via MCP para ver si ya hay una sesión activa.

3. Pregunta al desarrollador:
   - ¿En qué iniciativa / tarea estás trabajando ahora?
   - ¿Qué estabas intentando lograr en esta sesión?
   - ¿Cuál es el siguiente paso concreto para el próximo agente?
   - ¿Alguna decisión tomada en esta sesión que no hayas registrado aún?

4. Llama `checkpoint()` via MCP con los datos recogidos:
   ```
   checkpoint(
     initiative=<iniciativa>,
     task_id=<tarea>,
     intent=<qué estabas haciendo>,
     next_steps=<siguiente paso concreto>,
     context_notes=<decisiones y gotchas>
   )
   ```

5. Si hay decisiones sin registrar, llama `log_decision()` para cada una.

6. Lee `spec-native/SESSION.md` con `read_context('session')` y muéstraselo al desarrollador para confirmar.

7. Confirma:
   > "Traspaso listo. El próximo agente puede llamar `resume()` para continuar desde aquí."

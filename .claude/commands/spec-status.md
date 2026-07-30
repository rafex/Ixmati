Muestra el estado actual del proyecto SpecNative en una vista rápida y accionable.

## Proceso

1. Llama `resume()` via MCP — muestra si hay trabajo activo de otra sesión.

2. Llama `status()` via MCP — muestra specs activas y conteo de tareas por estado.

3. Llama `health_check()` via MCP — muestra alertas de documentación.

4. Resume en 3-5 líneas concisas:
   - Qué está bien (specs activas, docs completos)
   - Qué necesita atención (vacíos, tareas bloqueadas, sesión pendiente)
   - La acción más urgente a tomar

**Salida esperada:**
```
Estado SpecNative — [nombre del proyecto]

Sesión activa: [sí/no — qué iniciativa y tarea]
Specs: [N activas, N en draft, N completadas]
Tareas: [N done, N in_progress, N todo, N blocked]

⚠️  Alertas:
  - [doc X tiene secciones vacías]
  - [spec Y sin tasks vinculadas]

→ Acción sugerida: [la más urgente]
```

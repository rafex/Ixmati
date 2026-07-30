# MCP.md — SpecNative MCP Server v0.6

El servidor MCP de SpecNative expone el repositorio como **recursos**, **herramientas**
y **prompts** para que cualquier agente compatible con MCP pueda trabajar en modo
spec-first sin navegar manualmente el árbol de archivos.

**v0.6 agrega comandos nativos para Claude Code, OpenCode y Codex**, plus
herramientas de definición guiada (`health_check`, `suggest_next`, `refine_document`,
`init_project_guided`) para que adoptar SpecNative tome minutos, no horas.

## Comandos por agente

Instalados automáticamente en tu repositorio. Disponibles desde el primer día.

### Claude Code — slash commands

| Comando | Descripción |
|---------|-------------|
| `/spec-init` | Wizard guiado: entrevista al desarrollador y llena los documentos core |
| `/spec-update` | Detecta vacíos, sugiere refinamientos, actualiza iterativamente |
| `/spec-status` | Vista rápida: sesión activa, specs, tareas, alertas |
| `/spec-handoff` | Genera traspaso estructurado para el siguiente agente |

Archivos en `.claude/commands/spec-*.md`.

### OpenCode — prompts integrados

Disponibles en el menú de prompts de OpenCode (configurados en `opencode.json`):

| Prompt | Descripción |
|--------|-------------|
| `spec-init` | Initialize SpecNative — guided project setup |
| `spec-update` | Update SpecNative docs — detect gaps, refine |
| `spec-status` | Quick SpecNative health check |
| `spec-handoff` | Generate handoff for next agent |

### Codex CLI — prompts en codex.toml

```bash
codex --prompt spec-init     # guided project setup
codex --prompt spec-update   # detect gaps and refine
codex --prompt spec-status   # health check
codex --prompt spec-handoff  # generate handoff
```

### CLI sin agente

```bash
python3 specnative.py init              # wizard interactivo en terminal
python3 specnative.py update            # health check + refinamiento guiado
python3 specnative.py update --doc stack  # actualizar solo un documento
```

## Instalación

El instalador de SpecNative descarga el servidor MCP y crea un entorno virtual
aislado con todas sus dependencias automáticamente:

```
.specnative/specnative_mcp.py   ← servidor MCP
.specnative/.venv/              ← entorno virtual con mcp instalado
```

Si necesitas reinstalar o actualizar el servidor:

```bash
python3 install.py --reinstall --target /ruta/a/tu/repo
```

---

## Configuración por agente

El servidor usa el Python del venv aislado en `.specnative/.venv/`.
Reemplaza `/ruta/a/tu/proyecto` con la ruta absoluta real de tu repositorio.

### Claude Code

```bash
# Desde la raíz de tu proyecto:
claude mcp add specnative \
  "$(pwd)/.specnative/.venv/bin/python3" "$(pwd)/.specnative/specnative_mcp.py" \
  -- --repo "$(pwd)"
```

O agrega a `.claude/mcp_settings.json` (proyecto) o `~/.claude/mcp_settings.json` (global):

```json
{
  "mcpServers": {
    "specnative": {
      "command": "/ruta/a/tu/proyecto/.specnative/.venv/bin/python3",
      "args": [
        "/ruta/a/tu/proyecto/.specnative/specnative_mcp.py",
        "--repo", "/ruta/a/tu/proyecto"
      ]
    }
  }
}
```

### Claude Desktop

Agrega a `claude_desktop_config.json`
(`~/Library/Application Support/Claude/` en macOS,
`%APPDATA%\Claude\` en Windows):

```json
{
  "mcpServers": {
    "specnative": {
      "command": "/ruta/a/tu/proyecto/.specnative/.venv/bin/python3",
      "args": [
        "/ruta/a/tu/proyecto/.specnative/specnative_mcp.py",
        "--repo", "/ruta/a/tu/proyecto"
      ]
    }
  }
}
```

### OpenCode

Generado automáticamente en `opencode.json` durante la instalación.
Usa la clave `command` del schema de OpenCode (no `prompts` — esa clave no existe).
La clave `instructions` hace que OpenCode cargue `AGENTS.md` automáticamente en cada sesión.

```json
{
  "$schema": "https://opencode.ai/config.json",
  "instructions": [
    "AGENTS.md",
    "spec-native/README.md"
  ],
  "mcp": {
    "specnative": {
      "type": "local",
      "enabled": true,
      "command": [
        "./.specnative/.venv/bin/python3",
        "./.specnative/specnative_mcp.py"
      ]
    }
  },
  "command": {
    "spec-init": {
      "description": "Initialize SpecNative — guided project setup",
      "template": "Use the specnative MCP server. Call health_check() to see which spec-native/ documents are empty. Interview the developer and fill PRODUCT.md, STACK.md, ARCHITECTURE.md, CONVENTIONS.md and COMMANDS.md using update_section() or refine_document(). Finish by suggesting start_initiative() for the first spec."
    },
    "spec-update": {
      "description": "Update SpecNative docs — detect gaps, refine iteratively",
      "template": "Use the specnative MCP server. Call health_check() and suggest_next() to identify gaps. Ask the developer what to refine today, then use update_section() or refine_document() to update the documents."
    },
    "spec-status": {
      "description": "Quick SpecNative project health check",
      "template": "Use the specnative MCP server. Call resume(), status() and health_check(). Summarize in 5 lines what is healthy and what needs attention."
    },
    "spec-handoff": {
      "description": "Generate structured handoff for next agent",
      "template": "Use the specnative MCP server. Ask the developer what they were doing and what the next step is. Call checkpoint() with the gathered info, then log_decision() for any unrecorded decisions. Confirm with read_context('session')."
    }
  }
}
```

> **Nota:** La clave `instructions` es exclusiva de OpenCode — le indica qué archivos
> incluir como contexto en cada sesión. `AGENTS.md` se carga automáticamente sin
> necesidad de pedírselo al agente.

### Codex CLI

Agrega a `~/.codex/config.toml` (global) o `codex.toml` (raíz del proyecto):

```toml
[mcp_servers.specnative]
command = "/ruta/a/tu/proyecto/.specnative/.venv/bin/python3"
args = [
  "/ruta/a/tu/proyecto/.specnative/specnative_mcp.py",
  "--repo", "/ruta/a/tu/proyecto"
]
type = "stdio"
```

### Variable de entorno (alternativa universal)

```bash
export SPECNATIVE_REPO=/ruta/a/tu/proyecto
.specnative/.venv/bin/python3 .specnative/specnative_mcp.py
```

### Transporte SSE (agentes remotos)

```bash
.specnative/.venv/bin/python3 .specnative/specnative_mcp.py \
  --repo /ruta/al/proyecto \
  --transport sse \
  --port 8765
```

---

## Recursos disponibles

| URI                          | Documento                              |
|------------------------------|----------------------------------------|
| `spec://agents`              | `AGENTS.md` — contrato operativo       |
| `spec://session`             | `spec-native/SESSION.md` — estado activo |
| `spec://context/product`     | `spec-native/PRODUCT.md`               |
| `spec://context/architecture`| `spec-native/ARCHITECTURE.md`          |
| `spec://context/stack`       | `spec-native/STACK.md`                 |
| `spec://context/conventions` | `spec-native/CONVENTIONS.md`           |
| `spec://context/commands`    | `spec-native/COMMANDS.md`              |
| `spec://context/decisions`   | `spec-native/DECISIONS.md`             |
| `spec://context/roadmap`     | `spec-native/ROADMAP.md`               |
| `spec://context/traceability`| `spec-native/TRACEABILITY.md`          |
| `spec://pipelines/ci`        | `spec-native/pipelines/CI.md`          |
| `spec://pipelines/cd`        | `spec-native/pipelines/CD.md`          |
| `spec://schema`              | `.specnative/SCHEMA.md`                |

---

## Herramientas disponibles

### Consulta

| Herramienta                  | Descripción                                                    |
|------------------------------|----------------------------------------------------------------|
| `status()`                   | Estado de cada spec y conteo de tareas por estado              |
| `validate()`                 | Verifica que existan todos los archivos obligatorios           |
| `list_specs()`               | Lista specs con ID, estado y owner                             |
| `list_tasks(initiative)`     | Lista tareas de una iniciativa con estados                     |
| `read_spec(initiative)`      | Lee el contenido de una spec                                   |
| `read_context(document)`     | Lee un documento de contexto por nombre corto                  |
| `export_index()`             | Exporta specs y task files con metadata TOML como JSON         |
| `context_snapshot(initiative?)` | Dump completo de contexto para onboarding de nuevo agente  |

### Continuidad multi-agente (v0.5)

| Herramienta                               | Descripción                                                    |
|-------------------------------------------|----------------------------------------------------------------|
| `resume()`                                | Lee SESSION.md y genera resumen de continuidad                 |
| `checkpoint(initiative, task_id, intent, next_steps, context_notes?, agent_name?)` | Guarda estado antes de pausar |
| `update_task(initiative, task_id, state, notes?)` | Actualiza estado de tarea en TASKS.md              |
| `log_decision(title, context, decision, consequences)` | Append rápido a DECISIONS.md              |

### Definición y salud del proyecto (v0.6)

| Herramienta                               | Descripción                                                    |
|-------------------------------------------|----------------------------------------------------------------|
| `health_check()`                          | Escanea spec-native/ y reporta vacíos, docs faltantes, sesión obsoleta |
| `suggest_next()`                          | Sugiere las 3 acciones más impactantes basado en estado actual |
| `refine_document(document, what_changed, new_content)` | Actualiza un documento con nuevo contenido     |

---

## Prompts disponibles

### Definición del proyecto (v0.6)

| Prompt                                    | Descripción                                              |
|-------------------------------------------|----------------------------------------------------------|
| `init_project_guided(name, problem, users, goals, non_goals, stack, arch, conv, cmds)` | Llena los documentos core con contenido real del proyecto |

### Flujo de iniciativas

| Prompt                                    | Descripción                                              |
|-------------------------------------------|----------------------------------------------------------|
| `start_initiative(name, problem)`         | Inicia una nueva iniciativa spec-driven                  |
| `plan_tasks(initiative)`                  | Deriva el plan de tareas desde una spec                  |
| `implement_task(initiative, task_id)`     | Implementa una tarea específica                          |
| `review_against_spec(initiative)`         | Revisa implementación contra criterios de aceptación     |
| `handoff(summary, next_steps, decisions?)` | Genera traspaso estructurado para el siguiente agente   |
| `record_decision(title, ctx, dec, cons)`  | Registra una decisión persistente en DECISIONS.md        |
| `close_initiative(initiative)`            | Cierra la iniciativa y actualiza trazabilidad            |

---

## Flujo multi-agente

```
Agente A (Claude Code) implementa TASK-AUTH-0002:
  → update_task('authentication', 'TASK-AUTH-0002', 'in_progress')
  → ... trabaja ...
  → Se acaban los tokens. Llama checkpoint antes de cerrar:
  → checkpoint(
       initiative='authentication',
       task_id='TASK-AUTH-0002',
       intent='Implementando middleware JWT',
       next_steps='1. Agregar endpoint /refresh\n2. Escribir tests de integración',
       context_notes='JWT secret en env AUTH_SECRET. No hardcodear.'
     )

Agente B (Codex) entra al repo:
  → Lee AGENTS.md
  → resume()
  ← "Task TASK-AUTH-0002 in_progress. Intent: Implementando middleware JWT.
     Next: 1. Agregar endpoint /refresh..."
  → Continúa sin fricción
```

---

## Separación de responsabilidades

El servidor MCP es **infraestructura del framework**, no contenido del proyecto:

- Los documentos del proyecto viven en `spec-native/`.
- El servidor MCP lee y escribe esos documentos mediante herramientas tipadas.
- Las reglas de ownership siguen siendo las de `AGENTS.md` y `SCHEMA.md`.
- `.specnative/specnative_mcp.py` y `.specnative/.venv/` pueden agregarse a
  `.gitignore` si prefieres no versionarlos; o commitearlos si quieres que el
  equipo use exactamente la misma versión.

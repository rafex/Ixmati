## Herramientas de desarrollo

Ixmati separa **build** (make) de **tasks** (just). Ver DEC-0021.

### Makefile — construir artefactos

`make` compila, genera código y ensambla distribuciones. NO invoca a `just`.

| Comando | Descripción |
|---|---|
| `make build` | Compilar debug |
| `make build-release` | Compilar release |
| `make proto` | Generar código desde `.proto` |
| `make docker` | Construir imágenes Docker |
| `make dist` | Ensamblar `dist/` con checksums |
| `make clean` | Limpiar `target/` y `dist/` |

### Justfile — task manager

`just` ejecuta tareas de desarrollo. Puede invocar a `make`.

| Comando | Descripción |
|---|---|
| `just doctor` | Verificar herramienta |
| `just env-up/down` | Entorno de desarrollo |
| `just test` | Todos los tests |
| `just fmt` | Formatear código |
| `just clippy` | Linter Rust |
| `just quality` | fmt + clippy + boundary + validate |
| `just hooks-install` | Instalar git hooks |
| `just docs-serve` | mdBook con live reload |
| `just ci-pr` | Checks de PR |
| `just ci-main` | Checks de main (con smoke + coverage) |

### Estructura de helpers

```
helpers/
├── make/          ← módulos .mk incluidos por Makefile
├── just/          ← recetas .just importadas por Justfile
├── shell/         ← scripts bash (lib.sh, preflight, wait_for, etc.)
└── python/        ← herramientas Python con uv (>= 3.12)
```

`Makefile` y `Justfile` en la raíz son thin: solo incluyen/importan de `helpers/`. La lógica compartida vive exclusivamente en `helpers/`.

### Verificar el contrato make ≠ just

```bash
just boundary
```

Este comando ejecuta `helpers/python/lint_tool_boundary.py`, que escanea `Makefile` y `helpers/make/*.mk` en busca de invocaciones a `just`. Si encuentra alguna, falla.

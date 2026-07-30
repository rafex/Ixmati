# SPEC.md

```toml
artifact_type = "spec"
id = "SPEC-TOOL-0001"
state = "active"
owner = "team-core"
created_at = "2026-07-29"
updated_at = "2026-07-29"
replaces = "none"
related_tasks = [
  "TASK-TOOL-0001", "TASK-TOOL-0002", "TASK-TOOL-0003", "TASK-TOOL-0004",
  "TASK-TOOL-0005", "TASK-TOOL-0006", "TASK-TOOL-0007", "TASK-TOOL-0008",
  "TASK-TOOL-0009", "TASK-TOOL-0010", "TASK-TOOL-0011", "TASK-TOOL-0012",
  "TASK-TOOL-0013", "TASK-TOOL-0014"
]
related_decisions = [
  "DEC-0021", "DEC-0022", "DEC-0023", "DEC-0024", "DEC-0025", "DEC-0026", "DEC-0027"
]
artifacts = ["Makefile", "Justfile", "helpers/*", ".githooks/*", "tests/*", "docs/*", ".github/workflows/*", ".coverage-floor"]
```

## Metadata

- **ID**: SPEC-TOOL-0001
- **Estado**: `active`
- **Owner**: team-core
- **Fecha de creación**: 2026-07-29
- **Reemplaza**: `none`

## Resumen

Establecer la infraestructura de tooling del proyecto: Makefile para builds, Justfile como task manager, helpers modulares, git hooks versionados, estructura de tests con tiers, ratchet de cobertura, documentación con mdBook y pipeline CI.

## Problema

El proyecto necesita tooling estandarizado antes de empezar a escribir código de producción. Sin un harness de build/test/lint/fmt automatizado, cada contribuidor usa comandos distintos, la cobertura no se monitorea, y los hooks de calidad no se aplican consistentemente.

## Objetivo

Todo contribuidor ejecuta `just doctor`, `just test`, `just quality` y obtiene el mismo resultado. El CI ejecuta `just ci-pr`/`just ci-main`. La cobertura tiene un piso versionado que solo sube. Los git hooks se instalan con `just hooks-install`.

## Requisitos funcionales

- **RF-1**: `make build` compila el workspace.
- **RF-2**: `just test` ejecuta unitarios + integración + smoke.
- **RF-3**: `just quality` ejecuta fmt-check + clippy + boundary + validate.
- **RF-4**: `just hooks-install` configura `core.hooksPath → .githooks`.
- **RF-5**: `just doctor` reporta herramientas faltantes y versiones.
- **RF-6**: `make` nunca invoca a `just`; `lint_tool_boundary.py` lo verifica.
- **RF-7**: `just docs-serve` levanta mdBook con live reload.
- **RF-8**: `just ci-pr` ejecuta verificaciones de PR (fmt, clippy, boundary, unit tests).
- **RF-9**: `just ci-main` añade integration, smoke y coverage gate.
- **RF-10**: `.coverage-floor` contiene el piso de cobertura; `just test-cov-gate` falla si la cobertura baja.
- **RF-11**: `just test-unit` falla con el bootstrap TDD en rojo (7 crates con test `assert_eq!(2+2, 5)`).

## Criterios de aceptación

- **CA-1**: `just doctor` se ejecuta sin errores (advierte faltantes, no bloquea).
- **CA-2**: `make build` compila todos los crates sin errores.
- **CA-3**: `just test-unit` falla con 7 tests en rojo (bootstrap TDD) — no puede pasar hasta que se implemente algo.
- **CA-4**: `just boundary` pasa (make no invoca just).
- **CA-5**: `just quality` pasa.
- **CA-6**: `just docs-serve` compila el libro sin errores.
- **CA-7**: `just hooks-install && git config core.hooksPath` devuelve `.githooks`.
- **CA-8**: `.github/workflows/ci.yml` es sintácticamente válido.

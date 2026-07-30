# CI.md — Continuous Integration Pipeline

## Visión general

Cada PR debe pasar gates automatizados antes de merge. Cada push a main debe pasar gates extendidos. Los gates se ejecutan vía `just` para consistencia entre local y CI.

## Gates de PR (`just ci-pr`)

Ejecutados en cada push a PR y en cada commit de PR:

| Gate | Comando | Descripción |
|---|---|---|
| Formato | `just fmt-check` | `cargo fmt --check` |
| Clippy | `just clippy` | `cargo clippy --all-targets -- -D warnings` |
| Boundary | `just boundary` | Verifica que make no invoca a just |
| Config | `just validate-config` | Valida stores.toml, projections.toml |
| Commit msg | `commit_msg_lint.py --ci` | Verifica Conventional Commits en el PR |
| Unit tests | `just test-unit` | Tests unitarios (sin integración ni smoke) |

## Gates de main (`just ci-main`)

Ejecutados en push a `main`:

Incluye todos los gates de PR, más:

| Gate | Comando | Descripción |
|---|---|---|
| Integration tests | `just test-integration` | Tests de integración (crate Rust) |
| Smoke tests | `just test-smoke` | Tests de caja negra (pytest) |
| Coverage gate | `just test-cov-gate` | Cobertura no baja del piso (`.coverage-floor`) |

## Workflow GitHub Actions

Archivo: `.github/workflows/ci.yml`

Triggers:
- `pull_request` → gates de PR
- `push` a `main` → gates de main

Jobs:
1. `ci-pr`: `just ci-pr`
2. `ci-main`: `just ci-main` (solo en push a main)

## Contenedores (CI)

- Imagen base: `rust:1.85-slim-bookworm`
- Builder compartido: `ixmati-builder:local`
- Compose de test: `containers/compose/test.yaml` con Mosquitto ephemeral
- Sin persistencia entre ejecuciones de CI

## Métricas de CI

- Tiempo objetivo de CI-PR: < 5 minutos
- Tiempo objetivo de CI-main: < 15 minutos

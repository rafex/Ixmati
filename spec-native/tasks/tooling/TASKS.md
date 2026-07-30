# TASKS.md

```toml
artifact_type = "task_file"
initiative = "tooling"
spec_id = "SPEC-TOOL-0001"
owner = "team-core"
state = "in_progress"
```

## Metadata

- **Iniciativa**: tooling
- **Spec relacionada**: SPEC-TOOL-0001
- **Owner**: team-core
- **Estado general**: `in_progress`

## Tareas

### TASK-TOOL-0001 — helpers/python con uv, Python 3.12, pytest

```toml
id = "TASK-TOOL-0001"
title = "Configurar helpers/python con uv, Python >= 3.12 y pytest"
state = "done"
owner = "team-core"
dependencies = []
expected_files = ["helpers/python/pyproject.toml", "helpers/python/.python-version"]
validation = ["uv run python -c 'import sys; assert sys.version_info >= (3,12)'"]
```

### TASK-TOOL-0002 — helpers/shell/lib.sh + preflight.sh

```toml
id = "TASK-TOOL-0002"
title = "Implementar helpers/shell/lib.sh y preflight.sh (doctor)"
state = "done"
owner = "team-core"
dependencies = []
expected_files = ["helpers/shell/lib.sh", "helpers/shell/preflight.sh", "helpers/shell/wait_for.sh", "helpers/shell/mosquitto_dev.sh", "helpers/shell/kill9_writer.sh", "helpers/shell/litestream_restore.sh", "helpers/shell/sqlite_integrity.sh"]
validation = ["preflight.sh detecta herramientas y versiones", "lib.sh funciones log/ok/warn/err/die/require"]
```

### TASK-TOOL-0003 — Contrato make≠just + lint_tool_boundary.py

```toml
id = "TASK-TOOL-0003"
title = "Implementar lint_tool_boundary.py y sus tests (TDD)"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0001"]
expected_files = ["helpers/python/lint_tool_boundary.py", "helpers/python/tests/test_tooling.py"]
validation = ["lint_tool_boundary.py pasa con Makefile actual", "test_tooling.py::TestLintToolBoundary pasa"]
```

### TASK-TOOL-0004 — Makefile thin + helpers/make/*.mk

```toml
id = "TASK-TOOL-0004"
title = "Implementar Makefile y modulos helpers/make/"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0002", "TASK-TOOL-0003"]
expected_files = ["Makefile", "helpers/make/common.mk", "helpers/make/rust.mk", "helpers/make/proto.mk", "helpers/make/docker.mk", "helpers/make/artifacts.mk"]
validation = ["make help muestra targets", "make build compila (necesita rustup)"]
```

### TASK-TOOL-0005 — Justfile thin + helpers/just/*.just

```toml
id = "TASK-TOOL-0005"
title = "Implementar Justfile y recetas helpers/just/"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0002", "TASK-TOOL-0003", "TASK-TOOL-0004"]
expected_files = ["Justfile", "helpers/just/dev.just", "helpers/just/test.just", "helpers/just/quality.just", "helpers/just/hooks.just", "helpers/just/docs.just", "helpers/just/ci.just", "helpers/just/release.just"]
validation = ["just -l muestra recetas", "just doctor se ejecuta"]
```

### TASK-TOOL-0006 — .githooks/ + just hooks-install

```toml
id = "TASK-TOOL-0006"
title = "Implementar .githooks/ y just hooks-install"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0001", "TASK-TOOL-0005"]
expected_files = [".githooks/_common.sh", ".githooks/pre-commit", ".githooks/commit-msg", ".githooks/pre-push"]
validation = ["hooks instalables via just hooks-install", "hooks ejecutables (chmod +x)"]
```

### TASK-TOOL-0007 — Workspace Cargo con 7 crates + bootstrap TDD en rojo

```toml
id = "TASK-TOOL-0007"
title = "Crear workspace Cargo y 7 crates con test rojo bootstrap"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0004", "TASK-TOOL-0005"]
expected_files = ["Cargo.toml", "crates/*/Cargo.toml", "crates/*/src/lib.rs"]
validation = ["just test-unit falla con 7 tests rojos (necesita rustup para verificar)", "cada crate tiene assert_eq!(2+2, 5)"]
```

### TASK-TOOL-0008 — tests/integration como crate miembro

```toml
id = "TASK-TOOL-0008"
title = "Crear tests/integration como crate del workspace"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0007"]
expected_files = ["tests/integration/Cargo.toml", "tests/integration/tests/bootstrap.rs"]
validation = ["cargo test -p ixmati-integration pasa (placeholder)"]
```

### TASK-TOOL-0009 — tests/smoke pytest + fixtures

```toml
id = "TASK-TOOL-0009"
title = "Crear tests/smoke con pytest y fixtures"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0001", "TASK-TOOL-0007"]
expected_files = ["tests/smoke/conftest.py", "tests/smoke/test_*.py", "tests/fixtures/stores.test.toml"]
validation = ["uv run pytest tests/smoke/ --collect-only no falla", "conftest.py fixture docker_compose_up definido"]
```

### TASK-TOOL-0010 — Ratchet de cobertura

```toml
id = "TASK-TOOL-0010"
title = "Implementar coverage_gate.py y .coverage-floor"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0007", "TASK-TOOL-0008"]
expected_files = ["helpers/python/coverage_gate.py", ".coverage-floor"]
validation = ["coverage_gate.py sin lcov => OK (sin datos)", ".coverage-floor contiene 0.0"]
```

### TASK-TOOL-0011 — Validadores

```toml
id = "TASK-TOOL-0011"
title = "Implementar validate_config.py, validate_envelope.py, commit_msg_lint.py"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0001"]
expected_files = ["helpers/python/validate_config.py", "helpers/python/validate_envelope.py", "helpers/python/commit_msg_lint.py"]
validation = ["validate_config.py corre sin errores", "commit_msg_lint.py valida Conventional Commits"]
```

### TASK-TOOL-0012 — docs/ con mdBook

```toml
id = "TASK-TOOL-0012"
title = "Crear docs/ con mdBook y just docs-serve/build"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0005"]
expected_files = ["docs/book.toml", "docs/src/SUMMARY.md", "docs/src/introduction.md"]
validation = ["mdbook build docs/ compila sin errores", "just docs-build funciona"]
```

### TASK-TOOL-0013 — Pipelines CI.md + CD.md

```toml
id = "TASK-TOOL-0013"
title = "Rellenar spec-native/pipelines/CI.md y CD.md con gates reales"
state = "todo"
owner = "team-core"
dependencies = ["TASK-TOOL-0005", "TASK-TOOL-0010"]
expected_files = ["spec-native/pipelines/CI.md", "spec-native/pipelines/CD.md"]
validation = ["CI.md describe los gates que just ci-pr/ci-main ejecutan", "CD.md describe el pipeline de release"]
```

### TASK-TOOL-0014 — GitHub Actions CI

```toml
id = "TASK-TOOL-0014"
title = "Crear .github/workflows/ci.yml"
state = "done"
owner = "team-core"
dependencies = ["TASK-TOOL-0013"]
expected_files = [".github/workflows/ci.yml"]
validation = ["Workflow sintacticamente valido", "PR trigger y push to main configurados"]
```

# Root Justfile — thin, solo importa recetas de helpers/just/
#
# Responsabilidad unica: task manager.
# Puede llamar a make. make NUNCA puede llamar a just. Ver DEC-0021.
#
# Uso: just -l | just test | just fmt | just hooks-install | just docs-serve

set positional-arguments := true

repo_root := `git rev-parse --show-toplevel 2>/dev/null || pwd`

import 'helpers/just/dev.just'
import 'helpers/just/test.just'
import 'helpers/just/quality.just'
import 'helpers/just/hooks.just'
import 'helpers/just/docs.just'
import 'helpers/just/ci.just'
import 'helpers/just/release.just'
import 'helpers/just/containers.just'
import 'helpers/just/installer.just'

# ── top-level aliases ──

# verifica el entorno de desarrollo
doctor:
    @{{repo_root}}/helpers/shell/preflight.sh

# compila via make
build: make-build
    @echo "build OK"

[private]
make-build:
    @make build

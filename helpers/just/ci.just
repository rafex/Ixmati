# helpers/just/ci.just — tasks de CI (llamadas desde GitHub Actions)

repo_root := `git rev-parse --show-toplevel 2>/dev/null || pwd`

# verificaciones de PR
[group('ci')]
ci-pr:
    @echo "[ci] PR checks..."
    @just fmt-check
    @just clippy
    @just boundary
    @just validate-config
    @uv run python {{repo_root}}/helpers/python/commit_msg_lint.py --ci
    @just test-unit
    @echo "[ci] PR OK"

# verificaciones de main (incluye coverage gate y smoke)
[group('ci')]
ci-main:
    @echo "[ci] main checks..."
    @just ci-pr
    @just test-integration
    @just test-smoke
    @just test-cov-gate
    @echo "[ci] main OK"

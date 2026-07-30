# helpers/just/quality.just — linting, formato, auditoria

# formatea codigo Rust
[group('quality')]
fmt:
    @echo "[fmt] cargo fmt..."
    cargo fmt --all
    @echo "[fmt] OK"

# verifica formato (sin modificar)
[group('quality')]
fmt-check:
    @echo "[fmt] verificando..."
    cargo fmt --all -- --check

# ejecuta clippy con warnings como error
[group('quality')]
clippy:
    @echo "[clippy] analizando..."
    cargo clippy --all-targets --all-features --workspace -- -D warnings

# auditoria de dependencias (cargo-deny)
[group('quality')]
audit:
    @echo "[audit] dependencias..."
    cargo deny check
    @echo "[audit] vulnerabilidades..."
    cargo audit

# verifica que make no invoque a just
[group('quality')]
boundary:
    @echo "[boundary] verificando contrato make ≠ just..."
    @uv run python {{repo_root}}/helpers/python/lint_tool_boundary.py

# valida archivos de config (stores.toml, projections.toml)
[group('quality')]
validate-config:
    @echo "[validate] config..."
    @uv run python {{repo_root}}/helpers/python/validate_config.py

# valida envelope de comando/evento contra schema
[group('quality')]
validate-envelope file="":
    @echo "[validate] envelope..."
    @uv run python {{repo_root}}/helpers/python/validate_envelope.py {{file}}

# ejecuta todas las verificaciones de calidad
[group('quality')]
quality:
    @just fmt-check
    @just clippy
    @just boundary
    @just validate-config
    @echo "[quality] todas las verificaciones OK"

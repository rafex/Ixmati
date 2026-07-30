# helpers/just/test.just — tareas de test

# ejecuta tests unitarios (in-source)
[group('test')]
test-unit filter="":
    @echo "[test] unitarios..."
    cargo test {{filter}} --lib --workspace

# ejecuta tests de integracion (crate tests/integration)
[group('test')]
test-integration filter="":
    @echo "[test] integracion..."
    cargo test {{filter}} -p ixmati-integration

# ejecuta tests de smoke (pytest caja negra)
[group('test')]
test-smoke filter="":
    @echo "[test] smoke..."
    @cd {{repo_root}} && uv run pytest tests/smoke/ -v {{filter}}

# ejecuta todos los tests
[group('test')]
test filter="":
    @just test-unit {{filter}}
    @just test-integration {{filter}}
    @just test-smoke {{filter}}

# genera reporte de cobertura
[group('test')]
test-cov:
    @echo "[test] cobertura (cargo-llvm-cov)..."
    cargo llvm-cov --workspace --html --output-dir target/coverage
    @echo "[test] reporte generado en target/coverage/index.html"
    @uv run python {{repo_root}}/helpers/python/coverage_gate.py

# ejecuta test de cobertura con gate (falla si baja del piso)
[group('test')]
test-cov-gate:
    @echo "[test] cobertura con gate..."
    cargo llvm-cov --workspace --lcov --output-path target/coverage.lcov
    @uv run python {{repo_root}}/helpers/python/coverage_gate.py --lcov target/coverage.lcov

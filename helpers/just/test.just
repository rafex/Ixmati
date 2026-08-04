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

# ejecuta tests de smoke (pytest caja negra, stack ya corriendo)
# usa SMOKE_HOST y SMOKE_SSH_HOST del entorno si existen
[group('test')]
test-smoke filter="":
    @echo "[test] smoke..."
    @cd {{repo_root}}/helpers/python && \
      SMOKE_HOST=${SMOKE_HOST:-localhost} SMOKE_SSH_HOST=${SMOKE_SSH_HOST:-} \
      PYTHONPATH={{repo_root}} \
      uv run pytest {{repo_root}}/tests/smoke/ -v {{filter}}

# levanta compose smoke + ejecuta pytest + teardown
# -- modo local:  requiere podman-tunnel-up
# -- modo remoto: SMOKE_SSH_HOST=<host> just smoke
[group('test')]
smoke filter="" ssh_host="${SMOKE_SSH_HOST:-}":
    @echo "[smoke] levantando stack smoke.yaml..."
    @if [ -n "{{ssh_host}}" ]; then \
      echo "[smoke] modo remoto → {{ssh_host}}"; \
      ssh -o ConnectTimeout=10 {{ssh_host}} "cd ~/Ixmati && podman compose -f containers/compose/smoke.yaml up -d"; \
      sleep 10; \
      ssh -fN -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 \
        -L 30310:localhost:30310 -L 30311:localhost:30311 {{ssh_host}} 2>/dev/null || true; \
    else \
      just podman-tunnel-status; \
      COMPOSE_FILE=smoke.yaml make containers-up; \
      sleep 15; \
    fi
    @echo "[smoke] ejecutando tests..."
    -@cd {{repo_root}}/helpers/python && \
      SMOKE_HOST=localhost \
      SMOKE_SSH_HOST={{ssh_host}} \
      PYTHONPATH={{repo_root}} \
      uv run pytest {{repo_root}}/tests/smoke/ -v {{filter}} --tb=short
    @echo "[smoke] derribando stack..."
    @if [ -n "{{ssh_host}}" ]; then \
      ssh -o ConnectTimeout=10 {{ssh_host}} "cd ~/Ixmati && podman compose -f containers/compose/smoke.yaml down -v"; \
      kill $$(lsof -nP -iTCP:30310 -sTCP:LISTEN -t 2>/dev/null) \
            $$(lsof -nP -iTCP:30311 -sTCP:LISTEN -t 2>/dev/null) 2>/dev/null || true; \
    else \
      COMPOSE_FILE=smoke.yaml make containers-down; \
    fi

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

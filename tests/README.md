# tests/

Tests externos al codigo fuente. Los tests unitarios viven in-source (`#[cfg(test)]`).

## Tiers

| Tier | Carpeta | Herramienta | Que prueba |
|---|---|---|---|
| Unit | `crates/*/src/` | `cargo test --lib` | Funciones, modulos, logica interna |
| Integration | `tests/integration/` | `cargo test -p ixmati-integration` | Cruza fronteras de crates (writer+core+cache, SQLite :memory:, Mosquitto real) |
| Smoke | `tests/smoke/` | `uv run pytest` | Caja negra contra docker compose. Crash kill -9, outbox, restore Litestream |

## Como anadir un test

- **Unitario**: `#[cfg(test)] mod tests { ... }` en el mismo archivo.
- **Integracion**: archivo en `tests/integration/tests/`. Accede a las APIs publicas de los crates.
- **Smoke**: archivo `tests/smoke/test_*.py`. Usa `conftest.py` para fixtures de docker compose.

## Ejecucion

```bash
just test-unit          # solo unitarios
just test-integration   # solo integracion
just test-smoke         # solo smoke
just test               # todos
just test-cov           # cobertura
```

## Estrategia de testing

Ixmati sigue TDD con tres tiers de test y un ratchet de cobertura.

### Tiers de test

| Tier | Carpeta | Herramienta | Qué prueba |
|---|---|---|---|
| Unit | `crates/*/src/` | `cargo test --lib` | Funciones, módulos, lógica interna |
| Integration | `tests/integration/` | `cargo test -p ixmati-integration` | Cruza fronteras de crates |
| Smoke | `tests/smoke/` | `uv run pytest` | Caja negra, crash kill -9, outbox, restore |

### TDD (Red → Green → Refactor)

1. **Red**: escribir un test que falle y verificar que falla.
2. **Green**: implementar el mínimo código para que pase.
3. **Refactor**: mejorar el código sin romper el test.

Cada tarea de implementación declara sus tests como criterio de entrada, no de salida.

### Ratchet de cobertura

El archivo `.coverage-floor` en la raíz contiene el piso mínimo de cobertura. Arranca en `0.0` y solo sube.

```bash
just test-cov-gate
```

Esto ejecuta `cargo llvm-cov`, calcula la cobertura actual y la compara contra `.coverage-floor`. Si bajó, falla. Si subió ≥ 0.5pp, sugiere editar el archivo para subir el piso.

### Ejecutar tests

```bash
just test-unit           # unitarios
just test-integration    # integración
just test-smoke          # smoke
just test                # todos
just test-cov            # cobertura + reporte HTML en target/coverage/
```

## Primeros pasos

### Prerrequisitos

- Rust toolchain (rustup, cargo, rustc >= 1.80)
- Mosquitto >= 2.0 (o Docker)
- just (task runner)
- uv (para Python helpers, >= 3.12)
- Opcional: litestream, mdbook, cargo-llvm-cov

### Verificar el entorno

```bash
just doctor
```

Esto ejecuta `helpers/shell/preflight.sh` y reporta qué falta.

### Instalar Python dependencies

```bash
cd helpers/python && uv sync
```

### Instalar git hooks

```bash
just hooks-install
```

### Compilar

```bash
make build
```

### Ejecutar tests

```bash
just test-unit          # unitarios (in-source)
just test-integration   # integracion (crate en tests/)
just test-smoke         # smoke (pytest caja negra)
just test               # todos
```

### Levantar entorno de desarrollo

```bash
just env-up             # Mosquitto en Docker
just env-down           # detener
```

### Documentación

```bash
just docs-serve         # mdBook con live reload en :3000
just docs-build         # compilar libro estatico
```

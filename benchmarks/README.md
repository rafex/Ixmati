# Comparativa de capacidad

Esta suite compara en el mismo host y con el mismo dataset:

- SQLite directo con `WAL`, `synchronous=NORMAL` y `busy_timeout=5000`.
- Ixmati completo: API, Mosquitto, writer, cache-server y projector.
- PostgreSQL 18 directo con `synchronous_commit=on`.

Los resultados propios se separan de las referencias oficiales de PostgreSQL.
El ejemplo de `pgbench` publicado en la documentación de PostgreSQL (896.967
TPS y 11.013 ms) es una salida ilustrativa, no una promesa de capacidad para
este hardware.

## Ejecución

En Debian amd64, con el repositorio en el SHA que se desea medir:

```bash
uv run --with 'psycopg[binary]==3.2.9' python benchmarks/runner.py \
  init-sqlite /tmp/ixmati-bench.sqlite
uv run --with 'psycopg[binary]==3.2.9' python benchmarks/runner.py \
  seed-sqlite /tmp/ixmati-bench.sqlite
```

El runner también admite `init-postgres`, `seed-postgres` y `load`. Para la
carga de Ixmati se usa `--engine http` contra el API publicado.

`run_suite.sh` levanta PostgreSQL 18, prepara el dataset, ejecuta las tasas
controladas y conserva los JSON de cada corrida, el manifiesto del host y la
configuración del motor.

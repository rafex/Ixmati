# Evidencia — comparativa SQLite, Ixmati y PostgreSQL

Estado: ejecución preparada; los valores medidos se agregan después de correr
`just benchmark-db` en Debian amd64.

## Metodología

- Host objetivo: Debian amd64 `192.168.3.175`.
- SQLite directo: WAL, `synchronous=NORMAL`, `busy_timeout=5000`.
- PostgreSQL medido: imagen oficial `postgres:18`, digest registrado en
  `raw/db-comparison-*/manifest.txt`, `synchronous_commit=on`.
- Ixmati: API, Mosquitto, writers, cache-server y projector.
- Dataset por ejecución: 10,000 usuarios y 100,000 pedidos por defecto.
- Warmup: 15s; medición: 30s; repeticiones: 3 por escenario.
- Una ejecución fría significa primer pase sin warmup sobre la base recién
  preparada; no equivale a vaciar forzosamente la page cache del kernel.
- Una ejecución es inválida si `client_saturated_ticks > 0`.

## Resultados medidos

| Motor | Escenario | Tasa objetivo | Throughput real | p50 | p95 | p99 | Errores | Durabilidad | Observaciones |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| Pendiente | Pendiente | — | — | — | — | — | — | — | — |

Los resultados crudos, manifiestos, JSON y logs se conservarán bajo
`spec-native/evidence/raw/db-comparison-<timestamp>/` durante la ejecución.

## Referencias oficiales de PostgreSQL

La documentación oficial de PostgreSQL 18 describe `pgbench` como el runner
para medir transacciones por segundo, latencia, fallos y scripts
personalizados: <https://www.postgresql.org/docs/current/pgbench.html>.

La salida ilustrativa publicada por esa documentación muestra 896.967 TPS y
11.013 ms de latencia media. Es un ejemplo de formato y no un resultado de
este host, por lo que no se mezcla con la tabla de resultados medidos.

El anuncio oficial de PostgreSQL 17 declara hasta 2x de mejora de throughput
de escritura en cargas de alta concurrencia por optimizaciones del WAL:
<https://www.postgresql.org/about/news/postgresql-17-released-2936/>.
También es contexto del proyecto PostgreSQL, no una medición de Debian ni una
capacidad atribuida a Ixmati.

## Límites

- SQLite directo e Ixmati no tienen la misma superficie: Ixmati incluye HTTP,
  MQTT, batching, idempotencia, outbox, cache y proyecciones.
- PostgreSQL directo se reporta como motor de base de datos, no como un
  producto equivalente a todo el pipeline de Ixmati.
- Pattern R mutable continúa siendo una limitación de Ixmati y no se declara
  como vista viva hasta implementar fan-out o invalidación inversa.

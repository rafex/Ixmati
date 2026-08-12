# Benchmarks

The benchmark suite compares SQLite directo, Ixmati and PostgreSQL with a
common dataset. `run_suite.sh` is the short comparison suite; it is not a
long-term capacity claim.

## Reusable JMeter soak

`ixmati-soak.jmx` sends durable `POST /write` requests with a shared,
rate-controlled throughput timer. It does not store response bodies, which
keeps a one-hour run bounded in memory. Use JMeter's `-l` option when a full
JTL is required:

```bash
jmeter -n -t benchmarks/ixmati-soak.jmx \
  -Jhost=127.0.0.1 -Jport=30000 -Jstore=default \
  -Jrate=150 -Jduration=3600 -Jconcurrency=200 \
  -Japi_key=ix-default-key \
  -l evidence/jmeter-150.jtl \
  -j evidence/jmeter-150.log
```

Repeat with `-Jrate=200` and a fresh Debian container. The rate is requests
per second; the JMX converts it to requests per minute for JMeter's
`ConstantThroughputTimer`. The test deliberately leaves `200`, `202`, `429`
and transport errors visible in the JTL instead of treating them all as
success. Combine the JTL with API/writer/MQTT snapshots and the five-minute
drain verification described in the load-testing runbook.

For a quick smoke run, use `-Jduration=30 -Jrate=20 -Jconcurrency=32`.

Para provisionar automáticamente un Debian nuevo por escalón, exponer API y
métricas del writer, y limpiar cada contenedor al terminar:

```bash
SOAK_RATES="150 200" DURATION=3600 DRAIN_SECONDS=300 \
  TEST_HOST=192.168.3.175 \
  helpers/shell/run_soak_debian.sh
```

El wrapper ejecuta cada tasa en un contenedor independiente. No se debe
interrumpir una corrida válida y luego reutilizar su contenedor como evidencia
del siguiente escalón.

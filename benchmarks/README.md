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

`run_soak_debian.sh` builds a separate `ixmati-soak-generator` image and runs
it with `--network host` on the Debian Podman host. The generator calls the API
through `127.0.0.1` by default (`PODMAN_HOST_IP`); request traffic therefore
does not traverse the operator Mac or the LAN interface.
The operator-side process only collects a snapshot every 15 minutes by using
Podman (`podman exec`, `podman logs` and `podman ps`). Set
`SNAPSHOT_INTERVAL` to change that interval. A generator container remains
detached while the test runs, so a transient loss of the operator's Podman
connection does not interrupt the load process.

El wrapper ejecuta cada tasa en un contenedor independiente. No se debe
interrumpir una corrida válida y luego reutilizar su contenedor como evidencia
del siguiente escalón.

## Comparación JSON, REST/Protobuf y gRPC

El binario `ixmati-protocol-bench` usa el mismo payload, `ack_mode=committed`,
dataset lógico, tasa controlada y concurrencia para las tres interfaces. En un
host Debian amd64 con las imágenes ya construidas:

```bash
PODMAN_CONNECTION=debian-server-wifi \
PODMAN_HOST_IP=127.0.0.1 \
RATES="40 100 150" DURATION=30 WARMUP=5 COOLDOWN=2 CONCURRENCY=200 \
  benchmarks/protocol_benchmark.sh
```

El script usa un contenedor generador con `--network host`, guarda un JSON por
protocolo y escalón, y captura métricas, servicios y recursos después de cada
corrida. `40/s` es el baseline con el throttle productivo; `100/s` y `150/s`
son diagnósticos. Una tasa con `429` o con saturación del cliente no se declara
capacidad sostenible. El cooldown evita que el warmup contamine la ventana del
rate limiter.

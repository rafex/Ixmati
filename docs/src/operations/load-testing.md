# Pruebas prolongadas de capacidad

El perfil productivo demostrado actualmente es 10 escrituras durables por
segundo por store. La
capacidad de 150/s y 200/s sólo puede declararse sostenible después de una
corrida de una hora por escalón en un contenedor Debian nuevo, con generador
externo rate-controlled y cinco minutos de drenado.

El XML reutilizable de JMeter es
[`benchmarks/ixmati-soak.jmx`](../../../benchmarks/ixmati-soak.jmx):

```bash
jmeter -n -t benchmarks/ixmati-soak.jmx \
  -Jhost=192.168.3.175 -Jport=30300 -Jrate=150 \
  -Jduration=3600 -Jconcurrency=200 \
  -l evidence-150.jtl -j evidence-150.log
```

El fallback sin JMeter es `helpers/python/rate_load.py`; conserva sólo un
reservoir acotado de latencias y snapshots JSONL. Los resultados deben incluir
respuestas `200`, `202`, `429`, errores, saturación del cliente, cola del
writer, outbox, último commit, MQTT, cache, WAL, memoria y estado de servicios.

Una corrida con cliente saturado o con tasa efectiva inferior al objetivo se
marca inconclusa, no como capacidad del servidor.

# Diagnóstico de atasco MQTT — 2026-08-11

## Alcance

Esta ejecución intenta reproducir el atasco histórico de la sesión MQTT bajo
sobrecarga extrema. No es un benchmark de capacidad: el criterio de validez
es que el generador no se sature y, en estas corridas, ese criterio no se
cumplió.

- Host: Debian amd64 mediante conexión Podman `debian-server`
- Contenedor: `ixmati-load-test`
- API: `192.168.3.175:30300`
- Métricas del writer: `192.168.3.175:30301`
- Probe: `helpers/shell/mqtt_stall_probe.sh`
- Servicios observados: API, writer, Mosquitto y SQLite
- Watchdog: `MQTT_WATCHDOG_TIMEOUT_MS=30000`

## Instrumentación

Cada ejecución guarda en un directorio de resultados:

- JSON y stderr del generador de tasa;
- muestras TSV cada cinco segundos con cola, ACKs, deferred, commits,
  PUBACKs, outbox, mensajes almacenados en Mosquitto y ticks del proceso;
- journal del writer.

El probe no mata ni reinicia servicios. El watchdog sólo se habilita para
observar si una pérdida real de progreso provoca una recuperación controlada.

## Corridas

### 1000 solicitudes/s, 60 s, 500 clientes, timeout 5 s

- `client_saturated_ticks=2980`;
- `completed=15966`, errores de timeout `2424`;
- throughput observado por el generador: aproximadamente `266/s`;
- respuestas: `200=8886`, `202=4656`;
- p50 `1317 ms`, p90 `2323 ms`, p99 `3711 ms`.

El writer siguió avanzando: el último commit aumentó, los ticks del proceso
continuaron y el journal registró batches procesados y eventos publicados.
No se observó un atasco silencioso del event loop en esta corrida.

### 1500 solicitudes/s, 120 s, 2000 clientes, timeout 10 s

- `client_saturated_ticks=3537`;
- `completed=39788`, errores `ConnectionResetError=6786` y `timeout=14440`;
- throughput observado por el generador: aproximadamente `332/s`;
- respuestas: `200=148`, `202=18414`;
- p50 `2107 ms`, p90 `3064 ms`, p99 `4063 ms`.

El writer continuó comprometiendo batches durante toda la ventana; no se
activó el watchdog. La cola de Mosquitto aumentó, pero esta corrida no permite
atribuir el crecimiento al writer porque el generador ya estaba saturado y
producía resets y timeouts.

Los artefactos crudos se conservaron durante la sesión en:

- `/tmp/ixmati-mqtt-stall-20260811T204631Z`
- `/tmp/ixmati-mqtt-stall-20260811T204852Z`

## Hallazgo de observabilidad

La primera captura mostró nombres como
`ixmati_mqtt_commands_acked_total_total`. OpenTelemetry añade el sufijo
Prometheus `_total` a los contadores; declararlo también en el nombre de
origen duplicaba el sufijo. Se corrigieron las definiciones para que el
exporter publique los nombres esperados (`*_total`) y se añadió una prueba
que rechaza cualquier `_total_total`.

## Conclusión

La verificación en vivo del artefacto mostró inicialmente el proceso anterior
como `/usr/local/bin/ixmati-writer (deleted)`: la reinstalación reemplazó el
archivo, pero `systemctl start` no reiniciaba un servicio ya activo. Se corrigió
el instalador para usar `systemctl restart` en orden de dependencias; después
del restart el endpoint publicó los nombres esperados y `just installer-test`
pasó instalación, reinstalación, round-trip y purga.

Estas corridas no reproducen el atasco MQTT histórico y no lo descartan. Sólo
demuestran que, bajo este perfil, el cliente se satura antes de producir una
medición válida y que el writer mantiene progreso durante la ventana
observada. `TASK-VAL-0035` permanece abierta.

La evidencia histórica de un atasco real, con CPU efectiva cercana a cero y
cola Mosquitto sin drenado, permanece en `DEC-0055`, `DEC-0056` y `DEC-0057`.
El siguiente intento válido requiere un generador externo que no se sature,
logs de Mosquitto en modo debug y snapshots de `/proc`, journal y `$SYS`.

## Corrida adicional: flood MQTT directo y traza de sistema

Se ejecutó una segunda reproducción sobre el mismo contenedor, sin el API como
generador principal: seis productores `mosquitto_pub -l -q 1` enviaron 180,000
comandos válidos al topic de comandos. El broker llegó a aproximadamente
100,000 mensajes almacenados, pero el writer siguió avanzando: aumentaron
continuamente `last_batch_commit_unix_seconds` y
`mqtt_commands_acked_total`, mientras la cola almacenada descendía.

Durante el drenado se capturó una traza de 20 segundos con `strace -f` y se
inspeccionaron los seis hilos del writer. El hilo principal permaneció
esperando el resultado del hilo SQLite (`futex`); el hilo `ixmati-write` sí
estuvo activo realizando lecturas y escrituras WAL. No aparecieron `fsync` o
`fdatasync` lentos ni errores de red. Al finalizar la captura, las métricas
acumuladas fueron:

| Segmento | Conteo | Suma | Promedio aproximado |
|---|---:|---:|---:|
| `batch_ack_duration_seconds` | 1,253 | 0.0565 s | 45 µs/batch |
| `sqlite_process_duration_seconds` | 1,204 | 497.4 s | 413 ms/batch |
| `cache_sync_duration_seconds` | 1,253 | 120.6 s | 96 ms/batch |
| `batch_cycle_duration_seconds` | 1,253 | 671.4 s | 536 ms/batch |

Esto no demuestra que el atasco histórico no pueda ocurrir, pero sí descarta
que el ACK MQTT sea el tramo dominante en la implementación durable actual.
El cuello observado en esta reproducción es el procesamiento SQLite bajo un
backlog grande; la espera del cache es secundaria y el ACK es despreciable.

### Corrección de alcance de DEC-0057

DEC-0057 validó `try_ack()` en el diseño anterior, donde el consumidor enviaba
el ACK al entrar el mensaje al canal. El commit `5cccb91` cambió el contrato:
el token de ACK viaja en `PendingCommand` y se confirma sólo después del
commit SQLite. En ese diseño durable, `MqttAck::ack()` volvió a llamar a
`rumqttc::Client::ack()` (bloqueante) desde `process_batch`; el `try_ack()` que
permanece en `consumer.rs` sólo cubre mensajes malformados. Por ello la
prueba histórica de DEC-0057 no cubre exactamente el camino durable actual y
no debe citarse como prueba de que ese camino ya fue sometido a un ACK
no-bloqueante.

El probe admite ahora `STRACE=1` y `STRACE_DURATION=<segundos>` para conservar
la traza de syscalls junto con los snapshots. Si `strace` no está instalado o
el kernel rechaza `ptrace`, el artefacto lo registra explícitamente.

## Causa raíz confirmada: acceso no indexado a `_idempotency`

La inspección del código encontró que `current_version()` ejecutaba:

```sql
SELECT MAX(version) FROM _idempotency
WHERE store = ?1 AND entity = ?2 AND key = ?3
```

La tabla sólo tenía la clave primaria `(store, idempotency_key)`. En la base
Debian del escenario de falla, con aproximadamente 60,000 filas, el plan era:

```text
SEARCH _idempotency USING INDEX sqlite_autoindex__idempotency_1 (store=?)
```

Esto obligaba a revisar todas las filas del store para cada comando. Se añadió
en `IdempotencyTracker::ensure_schema()` el índice covering:

```text
idx_idempotency_entity_key_version(store, entity, key, version)
```

La migración se probó sobre la misma base existente: después de reiniciar el
writer con el binario amd64 que contiene el cambio, `EXPLAIN QUERY PLAN` pasó a:

```text
SEARCH _idempotency USING COVERING INDEX idx_idempotency_entity_key_version
  (store=? AND entity=? AND key=?)
```

Con 50,000 filas preexistentes y 20,000 comandos adicionales enviados por
MQTT QoS 1 al mismo contenedor:

| Estado | Batches | Tiempo SQLite acumulado | Promedio | Resultado |
|---|---:|---:|---:|---|
| Sin índice, durante el crecimiento | — | — | hasta ~200 ms/batch | backlog y coste crecientes |
| Índice covering | 398 | 3.490 s | 8.8 ms/batch | 20,000/20,000 procesados; cola 0 |

La comparación usa ventanas distintas de la carga histórica, por lo que no se
presenta como un benchmark aislado. Sí demuestra el mecanismo causal: el plan
de acceso cambió y, en la misma base, el coste de SQLite cayó aproximadamente
20 veces mientras el writer drenaba el backlog. La consulta del índice se
protege con regresiones unitarias, incluida una migración de una tabla ya
existente.

El hallazgo es un error de diseño de acceso a datos que se manifestaba como un
atasco MQTT. No se cambió el transporte ni se atribuyó al `PUBACK`: las
mediciones anteriores mostraron unos 45 µs/batch de ACK frente a cientos de ms
de SQLite. El watchdog queda como defensa para una pérdida auténtica de
progreso, no como sustituto de esta corrección.

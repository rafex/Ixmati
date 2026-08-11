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

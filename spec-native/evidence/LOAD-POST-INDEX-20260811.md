# Validación de carga posterior al índice de `_idempotency` — 2026-08-11

## Identidad y entorno

- SHA publicado: `cc7b912e529f39a36b23ae0aba17da5c9c603602`
- Host: Debian amd64 mediante conexión Podman `debian-server`
- Contenedor: `ixmati-load-test`, imagen `localhost/ixmati-installer-test:latest`
- API: `192.168.3.175:30300`
- Métricas writer: `192.168.3.175:30301`
- Artefacto: `dist/ixmati-0.1.0-linux-amd64.tar.gz`
- Generador: `helpers/python/rate_load.py`, Python estándar, 200 clientes
- Watchdog: habilitado a 30 s
- Duración: 30 s por escalón; baseline de 60 s
- Throttle: igual al objetivo en la escalera; restaurado a 40/s después

El binario desplegado contiene la cadena
`idx_idempotency_entity_key_version`. El contenedor fue eliminado al terminar
la corrida y no se modificaron los contenedores `fhs-*` del host.

## Baseline productivo — 40/s

| Objetivo | Solicitudes | Aceptadas | Throughput aceptado | p50 | p90 | p99 | 429 | Saturación cliente |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 40/s | 2,400 | 2,339 | 39.0/s | 75.0 ms | 108.2 ms | 143.9 ms | 61 | 0 |

No hubo errores de red ni `202 PENDING`. Los `429` fueron respuestas del
rate-limiter al alcanzar exactamente el límite de 40 por ventana; no indican
saturación del generador. La latencia observada por el cliente se mide desde
el HTTP POST hasta la respuesta durable.

## Escalera rate-controlled

| Objetivo | Aceptadas | Throughput aceptado | p50 | p90 | p99 | 429 | Saturación cliente | Outbox final | Cola MQTT final |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 20/s | 572/600 | 19.1/s | 72.8 ms | 90.0 ms | 136.8 ms | 28 | 0 | 4 | 0 |
| 40/s | 1,171/1,200 | 39.0/s | 75.5 ms | 108.2 ms | 136.7 ms | 29 | 0 | 7 | 0 |
| 60/s | 1,763/1,800 | 58.8/s | 85.4 ms | 120.6 ms | 141.9 ms | 37 | 0 | 4 | 0 |
| 80/s | 2,352/2,400 | 78.4/s | 103.9 ms | 137.1 ms | 151.8 ms | 48 | 0 | 6 | 0 |
| 100/s | 2,932/3,000 | 97.7/s | 106.5 ms | 137.4 ms | 156.1 ms | 68 | 0 | 13 | 0 |
| 150/s | 4,286/4,500 | 142.9/s | 113.0 ms | 158.3 ms | 321.0 ms | 214 | 0 | 13 | 0 |
| 200/s | 5,800/6,000 | 193.3/s | 114.5 ms | 151.3 ms | 184.9 ms | 200 | 0 | 19 | 0 |

Todos los escalones son mediciones válidas de llegada: el generador mantuvo
`client_saturated_ticks=0`, sin errores ni timeouts. El throughput aceptado es
menor que el objetivo porque cada escalón fija el rate-limiter en ese mismo
valor y las ventanas producen algunos `429`; no se debe interpretar como una
caída del generador.

El rango sostenible conservador sigue siendo 40–100/s: el p99 se mantuvo por
debajo de 160 ms y las colas permanecieron controladas. 150/s mostró una cola
de latencia significativa. 200/s alcanzó 193.3/s aceptadas, pero requiere una
corrida más larga para declararlo sostenible; estos 30 segundos sólo
caracterizan el escalón.

## Estado del writer y recuperación

Al finalizar la escalera:

- `consumer_queue_depth=0`;
- `mqtt_consumer_connected=1`;
- `outbox_size` final por escalón entre 4 y 19;
- último commit de batch avanzó continuamente;
- servicios `mosquitto`, `ixmati-cache-server`, `ixmati-writer@default`,
  `ixmati-api` e `ixmati-projector` activos;
- health check HTTP `200`;
- el throttle temporal fue eliminado y el servicio API quedó restaurado al
  perfil productivo de 40/s.

En la corrida completa, el writer acumuló 3,943 batches, con 11.20 s de
procesamiento SQLite acumulado (2.84 ms/batch promedio) y 41.87 s de ciclo de
batch acumulado (10.62 ms/batch promedio). Son métricas acumuladas del
proceso, no percentiles de una sola solicitud.

## Conclusión

El índice elimina el crecimiento lineal del lookup de idempotencia y el
comportamiento observado ya no presenta el atasco aparente de MQTT. El perfil
productivo de 40/s queda dentro del guardrail histórico de p99 <250 ms en esta
ejecución. Aún falta una prueba prolongada de estabilidad a 150–200/s antes de
promover esos valores como capacidad sostenible.

La evidencia cruda local de esta ejecución se generó en
`/tmp/ixmati-post-index-cc7b912` y `/tmp/ixmati-staircase-cc7b912`; la tabla
anterior conserva los resultados reproducibles y las limitaciones
metodológicas relevantes.

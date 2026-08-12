# Evidencia Debian amd64 — comparación JSON, REST/Protobuf y gRPC

Fecha: 2026-08-12
SHA publicado: `46a039fb265846d710b17b42a6b1b880ca627619`
Host de validación: Debian remoto vía `debian-server-wifi`
Podman: `5.4.2`, arquitectura `amd64`

## Protocolo

Se reconstruyeron el builder y los servicios desde el SHA publicado. El
generador se ejecutó como contenedor independiente con `--network host`, por lo
que llamó a `127.0.0.1:30000` y `127.0.0.1:30100` dentro de Debian. Se usaron
`200` clientes, payload equivalente, `ack_mode=committed`, claves de
idempotencia únicas y cooldown de dos segundos después del warmup.

Comando del arnés:

```bash
PODMAN_CONNECTION=debian-server-wifi \
PODMAN_HOST_IP=127.0.0.1 \
METRICS_URL=http://192.168.3.143:30000/metrics \
RATES="40 100 150" DURATION=30 WARMUP=5 COOLDOWN=2 CONCURRENCY=200 \
  benchmarks/protocol_benchmark.sh
```

La ejecución reproducible corta del arnés quedó en
`spec-native/evidence/raw/protobuf-bench-20260812T181902Z/` (JSON, métricas,
servicios, recursos y manifiesto con SHA). Las corridas de 30 segundos se
capturaron en la sesión de validación del mismo SHA; el post-push smoke de 10
segundos confirmó nuevamente los tres caminos.

## Resultados

### Baseline productivo: 40 operaciones/s, 30 segundos

| Protocolo | Throughput exitoso | p50 ms | p95 ms | p99 ms | Éxitos | Rechazos | Cliente saturado |
|---|---:|---:|---:|---:|---:|---:|---:|
| REST/JSON | 39.1/s | 1.334 | 1.456 | 1.489 | 1173 (`200`) | 28 (`429`) | no |
| REST/Protobuf | 39.1/s | 1.312 | 1.445 | 1.500 | 1173 (`200`) | 28 (`429`) | no |
| gRPC | 39.1/s | 1.430 | 1.559 | 1.626 | 1173 (`COMMITTED`) | 28 (`RESOURCE_EXHAUSTED`) | no |

Los `429`/`RESOURCE_EXHAUSTED` son backpressure del límite productivo exacto
`MAX_WRITES_PER_WINDOW=40`; no fueron errores de red ni pérdida de escrituras.
La tasa aceptada fue 97.7% de la tasa ofrecida porque el generador intenta
exactamente 40/s contra una ventana deslizante de 40 operaciones. Por tanto,
este baseline demuestra el comportamiento del perfil productivo, pero no debe
presentarse como 99.5% de confirmación a una tasa ofrecida exactamente igual al
límite.

### Diagnóstico: 100 y 150 operaciones/s, 12 segundos

| Tasa | Protocolo | Throughput exitoso | p50 ms | p95 ms | p99 ms | Éxitos | Rechazos |
|---:|---|---:|---:|---:|---:|---:|---:|
| 100 | REST/JSON | 40.0/s | 0.765 | 1.427 | 1.489 | 480 | 721 (`429`) |
| 100 | REST/Protobuf | 40.0/s | 0.747 | 1.406 | 1.449 | 480 | 721 (`429`) |
| 100 | gRPC | 40.0/s | 0.837 | 1.496 | 1.563 | 480 | 721 (`RESOURCE_EXHAUSTED`) |
| 150 | REST/JSON | 40.0/s | 0.737 | 61.719 | 103.765 | 480 | 1321 (`429`) |
| 150 | REST/Protobuf | 40.0/s | 0.714 | 1.366 | 61.421 | 480 | 1321 (`429`) |
| 150 | gRPC | 40.0/s | 0.813 | 1.482 | 1.580 | 480 | 1321 (`RESOURCE_EXHAUSTED`) |

Estos escalones son diagnósticos de backpressure, no mediciones de capacidad
de 100/150/s: el throttle productivo impide que la carga excedente alcance el
writer. No hubo `202`, errores de transporte ni saturación del generador.

### Confirmación post-push

Contra las imágenes reconstruidas desde `46a039f`, una corrida adicional de 10
segundos a 40/s obtuvo `392/401` éxitos (`39.2/s`) en cada protocolo, con cero
saturación del cliente. Las latencias p99 fueron 1.493 ms (JSON), 1.491 ms
(REST/Protobuf) y 1.600 ms (gRPC).

## Estado del sistema

- `/health`: `overall=OK`; API, SQLite y Mosquitto reportaron estado OK.
- `ixmati_outbox_size{store="pedidos"}`: `0` al finalizar.
- `PRAGMA integrity_check` sobre `/data/pedidos.db`: `ok`.
- Filas `_outbox` sin publicar: `0`.
- Métricas y stats de API, writer, projector, cache-server y Mosquitto fueron
  capturadas por el arnés después de cada protocolo.
- El contador acumulado `queue_full` quedó en `8809` después de todas las
  corridas; representa rechazos deliberados de la carga diagnóstica y no se
  interpreta como pérdida.

## Conclusión

JSON, REST/Protobuf y gRPC llegan al mismo camino durable. En este entorno y
workload, REST/Protobuf reduce ligeramente el overhead de serialización frente
a JSON, mientras gRPC tiene latencia similar; ninguna interfaz aumenta la
capacidad durable del writer. La capacidad recomendada continúa siendo el
perfil productivo de 15 escrituras/s por store, con margen bajo el techo
observado de 40/s. Las pruebas prolongadas de 150/200/s y las
pruebas de stream con cliente lento siguen siendo trabajo independiente.

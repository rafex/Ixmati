# Perfil productivo 10/s — validación de una hora

## Identidad y configuración

- SHA publicado y probado: `6c38eb8`
- Host Podman: Debian amd64 mediante `debian-server-wifi`
- Stack: Compose multi-store, API, writer, cache, projector y Mosquitto
- Generador: contenedor Debian separado, en `--network host`
- Protocolo: REST/JSON, `ack_mode=committed`
- Tasa objetivo: `10/s`
- Duración de régimen: `3600 s`
- Warmup/cooldown del generador: `10 s` / `10 s`
- Concurrencia: `200`
- `BATCH_INTERVAL_MS=100`
- Timeout de confirmación durable: `2000 ms`
- Prefijo de claves: `prod10-1h-6c38eb8`

## Resultado del generador

```text
submitted_operations=36001
completed_operations=36001
successful_operations=36001
throughput=10.0002777778/s
client_saturated_ticks=0
valid_rate_controlled=true
responses=200:36001
errors=none
p50=121.806 ms
p95=212.421 ms
p99=212.856 ms
max=1208.015 ms
```

La corrida terminó con `exit=0`. No se observaron timeouts de confirmación
durable para el prefijo de la prueba. El máximo aislado superó el p99, pero no
alteró el cumplimiento del régimen: el p99 permaneció bajo el guardrail de
250 ms definido para este perfil.

## Verificaciones independientes

Después de finalizar el generador y dejar cinco minutos de drenado:

- Claves del prefijo en `_idempotency`: `36102` (incluye warmup; las `36001`
  operaciones del régimen están presentes).
- Filas `_outbox` con `published_at IS NULL`: `0`.
- `PRAGMA integrity_check`: `ok`.
- Timeouts `ack_mode=committed` del prefijo: `0`.
- Reinicios de API y writer durante la ejecución: `0`.
- Errores de API/writer, `database is locked` y watchdog en los logs de la
  ventana: `0`.

## Clasificación

El perfil de **10 escrituras durables por segundo por store** queda demostrado
como sostenible para esta configuración y este host Debian amd64. Es un perfil
operativo recomendado, no un SLO universal: debe conservarse la configuración
de admisión, batching y almacenamiento usada en la medición.

Esto no valida 150/s ni 200/s. Esas tasas continúan requiriendo un soak
independiente; tampoco convierte al despliegue single-host en alta
disponibilidad. La corrección validada en esta corrida mantiene todas las
actualizaciones SQLite (`payload`, `_idempotency`, `_outbox` y `published_at`)
en el hilo único dueño de la conexión.

# Control del perfil productivo: 10/s

- Fecha de ejecución: 2026-08-13 UTC
- SHA probado: `6fc80ab359505bfd29861c20b81d41d669e9bbb2`
- Imagen del generador: `localhost/ixmati-builder:6fc80ab`
- Entorno: Debian amd64 remoto, Podman, red host; generador externo al stack
- Escenario: REST/JSON, `ack_mode=committed`, `200` clientes, tasa controlada
  de `10/s`, warmup 10 s, cooldown 10 s, duración 300 s

## Resultado del generador

```json
{
  "target_rate": 10.0,
  "duration_seconds": 300.0,
  "concurrency": 200,
  "submitted_operations": 3001,
  "completed_operations": 3001,
  "successful_operations": 3001,
  "throughput_per_second": 10.003333333333334,
  "client_saturated_ticks": 0,
  "valid_rate_controlled": true,
  "responses": {"200": 3001},
  "errors": {},
  "p50_ms": 31.932697,
  "p95_ms": 122.39726,
  "p99_ms": 212.752902,
  "max_ms": 213.04504
}
```

Al finalizar, el generador había producido `3,001` claves del escenario;
SQLite devolvió `PRAGMA integrity_check = ok`, `_outbox` tenía `0` filas sin
publicar y las `3,102` claves medidas (incluyendo warmup) estaban presentes
en `_idempotency`. No hubo `202`, `429`, errores de transporte ni saturación
del generador.

Este artefacto es un **control de cinco minutos**, no el cierre de
`TASK-PROD-0001`. La hora completa debe repetirse contra el SHA que incluya
la recalibración a 10/s.

# Evidencia — cache y vistas materializadas

Fecha: 2026-08-11
Entorno: Debian amd64 remoto (`192.168.3.175`)
Stack: `multi-store.yaml`, Redb vía Unix socket, tres writers, API y projector.

## Correcciones validadas

- El API multi-store montaba los volúmenes SQLite como archivos. Se corrigió a
  `/data/<store>/<store>.db` para que el API pueda abrir las bases compartidas.
- `CacheClient` ahora reconecta el socket Unix una vez cuando el cache-server
  reinicia.
- `/health` resuelve brokers con hostname (`mosquitto:30200`) en lugar de caer
  silenciosamente a `127.0.0.1:1883`.

## E2E funcional

Comando:

```bash
E2E_EXTERNAL=1 E2E_HOST=192.168.3.175 E2E_API_PORT=30000 \
E2E_MQTT_PORT=30200 PYTHONPATH=$PWD \
uv run pytest tests/smoke/test_ecommerce.py -v --tb=short
```

Resultado: **5 passed**.

Validado:

- cache-aside devuelve `source=cache`;
- Pattern M (`usuarios_materializados`) copia `nombre` y `email`;
- Pattern R (`pedidos_con_usuario`) construye la vista inicial;
- idempotencia y concurrencia multi-store;
- actualización de usuario refresca cache-aside y Pattern M.

## Fallback y recuperación

Con cache-server detenido se confirmó una escritura `APPLIED` en SQLite. Tras
reiniciar cache-server:

```text
primera lectura:  source=sqlite
segunda lectura: source=cache
```

Esto valida fallback y repoblación sin reiniciar el API.

## Lectura rate-controlled

Generador: `helpers/python/read_rate_load.py`, concurrencia 300, 10 segundos
por combinación. Todas las respuestas fueron HTTP 200 con payload válido y
`client_saturated_ticks=0`, salvo el caso indicado.

| Tasa | Cache p50/p90/p99 | Pattern M p50/p90/p99 | Resultado |
|---:|---:|---:|---|
| 100/s | 24/31/57 ms | 26/60/172 ms | PASS |
| 250/s | 35/52/105 ms | 37/56/235 ms | PASS |
| 500/s | 42/53/99 ms | 42/55/121 ms | PASS |
| 1000/s | 59/173/710 ms | 60/111/201 ms | PASS con saturación del generador sólo en cache |

## Limitación encontrada

Pattern R no se actualiza automáticamente cuando cambia una entidad
referenciada. Un pedido inicialmente proyectado con el usuario `Antes` sigue
mostrando `Antes` después de actualizar el usuario a `Después`. La vista
inicial funciona, pero el comportamiento mutable requiere fan-out, índice
inverso, lookup en lectura o reproyección. Se registra como `TASK-VAL-0036` y
no se declara apto para relaciones mutables de lectura productiva.

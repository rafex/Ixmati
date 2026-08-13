# Perfil productivo candidato: 15/s — no sostenible

- Fecha de ejecución: 2026-08-12 UTC
- SHA probado: `6fc80ab359505bfd29861c20b81d41d669e9bbb2`
- Imagen del generador: `localhost/ixmati-builder:6fc80ab`
- Entorno: Debian amd64 remoto, Podman, red host; generador externo al stack
- Escenario: REST/JSON, `ack_mode=committed`, `200` clientes, tasa controlada
  de `15/s`, `BATCH_INTERVAL_MS=100`, cache post-commit

## Resultado

La corrida se detuvo de forma anticipada después de aproximadamente ocho
minutos porque dejó de cumplir el criterio de confirmación durable dentro de
la ventana operativa:

| Momento | Solicitudes observadas | `200` | `202/PENDING` | Medidas en `_idempotency` | Outbox sin publicar |
|---|---:|---:|---:|---:|---:|
| ~6 min | 5,828 | 5,652 | 176 | 5,832 | 4 |
| ~8 min | 7,296 | 6,844 | 448 | 7,296 | 2 |

SQLite devolvió `PRAGMA integrity_check = ok`. Las claves que recibieron
`202` terminaron comprometidas, por lo que no se observó pérdida de datos;
sin embargo, el API agotó el timeout de confirmación de 2 s mientras el
writer trabajaba pegado a su capacidad. El generador no fue el límite y no se
observaron reinicios ni errores del writer.

## Clasificación

`15/s` queda clasificado como **diagnóstico, no perfil productivo**. El límite
se redujo a `10/s` para recuperar margen; se requiere un soak de una hora del
nuevo perfil antes de declararlo estable.

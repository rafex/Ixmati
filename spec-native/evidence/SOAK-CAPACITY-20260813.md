# Evidencia de capacidad prolongada: 150/s y 200/s

- Fecha: 2026-08-13
- SHA probado: `df381f4c09af4d344a315874ace8e8c1be9b9079`
- Host Podman: `debian-server-wifi` (`192.168.3.143`), Debian amd64,
  Podman remoto 5.4.2
- Arnés: `helpers/shell/run_soak_debian.sh` +
  `benchmarks/soak_capacity.sh`
- Generador: contenedor separado `ixmati-soak-generator`, red host del
  bastion, `127.0.0.1`, 400 clientes, tasa controlada
- Configuración: throttle temporal `MAX_WRITES_PER_WINDOW=1000000`; el
  override se retiró al terminar y los contenedores se eliminaron

Los dos escenarios se detuvieron al demostrar que no podían cumplir el
criterio de capacidad sostenible. No son resultados de una hora completa ni
se presentan como throughput productivo.

## 150/s — fallo del sistema antes de saturar el generador

Evidencia cruda: `raw/soak-150-20260813T041852Z/`.

En aproximadamente 130 segundos el generador reportó:

| Métrica | Resultado |
|---|---:|
| Tasa objetivo | 150/s |
| Completadas | 19,493 |
| Enviadas | 19,506 |
| Throughput de ventana | 149.50/s |
| `client_saturated_ticks` | 0 |
| HTTP 200 | 17,468 |
| HTTP 202/PENDING | 1,274 |
| HTTP 429 | 751 |
| Errores de red | 0 |

La API registró `751` rechazos `outbox_backlog`; el outbox superó el umbral
configurado de 500. En el snapshot del writer, `consumer_queue_depth` llegó a
185 y `cache_sync_queue_depth` a 71. El proceso permaneció activo y continuó
progresando, pero la confirmación durable dejó de cumplir la ventana de 2 s.

Clasificación: **no sostenible como perfil productivo a 150/s**. El resultado
es válido para demostrar el primer límite operativo porque el generador no se
saturó; no demuestra pérdida de datos.

## 200/s — generador saturado, capacidad inconclusa

Evidencia cruda: `raw/soak-200-20260813T042342Z/`.

En aproximadamente 200 segundos el generador reportó:

| Métrica | Resultado |
|---|---:|
| Tasa objetivo | 200/s |
| Completadas | 39,996 |
| Enviadas | 40,012 |
| Throughput de ventana | 241.22/s (ventana de drenado del cliente) |
| `client_saturated_ticks` | 2,338 |
| HTTP 200 | 37,163 |
| HTTP 202/PENDING | 2,833 |
| Errores de red | 0 |

Al aparecer saturación del generador, el escalón deja de ser una medición
válida de capacidad del servidor. Los `202` muestran degradación del camino
durable, pero no se extrapola un throughput de 200/s.

Clasificación: **inconcluso/no sostenible como perfil productivo**; requiere
un generador con más capacidad o varios generadores antes de cuantificar el
techo exacto.

## Conclusión operativa

La capacidad recomendada no cambia: 10/s por store es el único perfil con una
hora completa validada (`36,001/36,001` respuestas 200, p99 212.86 ms, outbox
drenado e integridad SQLite correcta). El rango 40–120/s tiene evidencia corta
diagnóstica; 150/s activa backpressure y 200/s no tiene una medición de techo
válida. No se debe anunciar 150–200/s como capacidad productiva.

Esta ejecución tampoco sustituye las validaciones aún abiertas de backup S3
real, RPO/RTO, TLS/reverse proxy y cutover/rollback de stores.

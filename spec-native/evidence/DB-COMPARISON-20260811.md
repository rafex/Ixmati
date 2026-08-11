# Evidencia — comparativa SQLite, Ixmati y PostgreSQL

Estado: ejecutada en Debian amd64 el 2026-08-11.

## Alcance y trazabilidad

- Host: `bastion-alqrab`, Debian amd64, 8 CPU visibles, 15 GiB RAM.
- Dataset común: 1,000 usuarios y 10,000 pedidos.
- Duración: 3 s por medición, 1 s de calentamiento, 3 repeticiones.
- Tasas: lecturas 100/250/500/1000 ops/s; escrituras 20/40/60/80/100/150/200 ops/s.
- SQLite: WAL, `synchronous=NORMAL`, `busy_timeout=5000`.
- PostgreSQL: imagen `postgres:18`, digest
  `sha256:4b87d5343a0e499eea33b6c19ec152a3ed4a0d1a453eadd26ff415a406821a6e`,
  `synchronous_commit=on`.
- Ixmati: API, Mosquitto, writers, cache-server y projector; escrituras con
  `ack_mode=committed`.
- Baseline directo final: SHA `6186230`.
- Ixmati final: SHA `370ec09`; el commit posterior `87c7e14` sólo añade
  snapshots de la fase directa y no cambia el camino Ixmati medido.
- Todas las tasas de la tabla son válidas sólo cuando `client_saturated_ticks=0`.
  En Ixmati, `throughput` es operaciones exitosas; los `http_429` se cuentan
  aparte.

Los JSON completos y snapshots están versionados en
`spec-native/evidence/raw/db-comparison-20260811/`. El arnés conserva también
los resultados no válidos para mostrar el techo y la saturación del cliente.

## Resultados medidos — lecturas calientes

Latencias en milisegundos; `—` en observaciones significa que la tasa no se
acepta como capacidad sostenible por saturación del cliente.

| Motor | Operación | Objetivo | Éxito/s | p50 | p95 | p99 | Errores | Observación |
|---|---|---:|---:|---:|---:|---:|---:|---|
| SQLite directo | punto | 100 | 100 | 0.17 | 0.18 | 0.21 | 0 | válida |
| PostgreSQL 18 | punto | 100 | 100 | 2.41 | 2.73 | 3.14 | 0 | válida |
| Ixmati | cache-aside punto | 100 | 100 | 2.61 | 3.55 | 3.65 | 0 | válida |
| SQLite directo | relación | 500 | 500 | 0.83 | 0.96 | 1.11 | 0 | válida |
| PostgreSQL 18 | relación | 500 | 500 | 1.81 | 2.55 | 25.43 | 0 | inválida: cliente saturado |
| Ixmati | vista `pedidos_con_usuario` | 500 | 500 | 1.23 | 2.21 | 2.54 | 0 | válida |
| SQLite directo | punto | 1000 | 1000 | 0.18 | 0.21 | 0.26 | 0 | válida |
| PostgreSQL 18 | punto | 1000 | 1000 | 1.67 | 2.33 | 5.98 | 0 | inválida: cliente saturado |
| Ixmati | cache-aside punto | 1000 | 1000 | 0.80 | 1.20 | 1.64 | 0 | válida |
| SQLite directo | relación | 1000 | 1000 | 0.69 | 0.75 | 1.04 | 0 | válida |
| PostgreSQL 18 | relación | 1000 | 1000 | 1.77 | 2.47 | 7.51 | 0 | inválida: cliente saturado |
| Ixmati | vista `pedidos_con_usuario` | 1000 | 1000 | 0.83 | 1.27 | 1.56 | 0 | válida |

En este workload, la cache y la vista materializada de Ixmati sostuvieron las
tasas de lectura probadas sin errores ni saturación del generador. Esto no
demuestra que Pattern R mutable sea siempre fresco: esa limitación sigue
vigente y está documentada en DEC-0062.

## Resultados medidos — escrituras durables

| Motor | Objetivo | Éxito/s | p50 | p95 | p99 | Errores | Durabilidad / observación |
|---|---:|---:|---:|---:|---:|---:|---|
| SQLite directo | 20 | 20 | 0.70 | 0.85 | 2.06 | 0 | transacción con idempotencia y outbox |
| PostgreSQL 18 | 20 | 20 | 6.18 | 7.06 | 21.96 | 0 | commit síncrono, idempotencia y outbox |
| Ixmati | 20 | 20 | 2003.43 | 2003.95 | 2006.18 | 0 | 60/60 confirmadas; p99 incluye espera de commit |
| SQLite directo | 40 | 40 | 0.69 | 0.92 | 1.34 | 0 | válida |
| PostgreSQL 18 | 40 | 40 | 5.86 | 6.87 | 22.16 | 0 | válida |
| Ixmati | 40 | 39 | 2002.87 | 2003.63 | 2009.49 | 9 `429` | primer rechazo observable |
| SQLite directo | 60 | 60 | 0.68 | 0.87 | 1.68 | 0 | válida |
| PostgreSQL 18 | 60 | 60 | 5.91 | 6.63 | 20.04 | 0 | válida |
| Ixmati | 60 | 40 | 2002.31 | 2003.26 | 2018.58 | 180 `429` | limitado por throttle/backpressure |
| SQLite directo | 100 | 100 | 0.73 | 0.91 | 1.69 | 0 | válida |
| PostgreSQL 18 | 100 | 100 | 5.87 | 6.57 | 9.06 | 0 | válida |
| Ixmati | 100 | 40 | 2.48 | 2002.89 | 2003.93 | 540 `429` | throughput durable queda en ~40/s |
| SQLite directo | 200 | 200 | 0.66 | 0.87 | 1.01 | 0 | válida |
| PostgreSQL 18 | 200 | 200 | 5.23 | 5.86 | 7.19 | 0 | válida |
| Ixmati | 200 | 40 | 1.51 | 2002.23 | 2005.84 | 1440 `429` | no es capacidad sostenible |

La conclusión productiva es deliberadamente conservadora: el camino completo
de Ixmati confirma aproximadamente 40 escrituras durables por segundo bajo el
throttle configurado; ofrecer 60/s o más no aumenta el commit rate y sí genera
rechazos. SQLite y PostgreSQL directos son referencias de motor, no sustitutos
del pipeline HTTP/MQTT/outbox/cache.

## Interpretación de producto

Estos resultados demuestran que Ixmati es funcional como una capa durable de
escritura y aceleración de lecturas sobre SQLite, no que sea un motor SQL de
throughput bruto superior a SQLite o PostgreSQL. Los baselines directos tienen
menos capas y permiten estimar el costo de añadir API, MQTT, serialización del
writer, confirmación de `_idempotency`, outbox, cache y proyecciones.

La lectura es el lado fuerte del producto en este workload: el camino completo
sostuvo 1,000 operaciones/s cacheadas o proyectadas con p99 aproximado de
1.6 ms y sin saturación del generador. La escritura durable es el límite
actual: con el perfil productivo confirmó aproximadamente 40 escrituras/s;
por encima de ese nivel aparecieron `429`/pendientes sin aumentar el commit
rate. La p99 cercana a 2 s en `ack_mode=committed` debe tratarse como un
cuello de botella de rendimiento, no como un SLO cumplido.

La conclusión de producto es **beta viable para single-host/edge, escritura
moderada y alta fan-out de lectura**. Ixmati convierte la limitación de
escritor único de SQLite en un servicio durable, observable y con backpressure
explícito; no elimina esa limitación. No se deben presentar estos datos como
prueba de que Ixmati supera a SQLite/PostgreSQL, como soporte para 100–200
escrituras durables/s, ni como sustitución general de PostgreSQL. La prueba
determinista de crash entre PUBACK y `published_at` sigue pendiente.

## Operaciones adicionales

| Motor | Operación a 100/s | Éxito/s | p50 | p95 | p99 | Errores | Validez |
|---|---|---:|---:|---:|---:|---:|---|
| SQLite directo | update | 100 | 0.70 | 0.89 | 1.51 | 0 | válida |
| PostgreSQL 18 | update | 100 | 5.77 | 6.53 | 9.23 | 0 | válida |
| Ixmati | update | 35.33 | 3.35 | 2002.99 | 2003.56 | 435 `429` | cliente saturado |
| SQLite directo | idempotencia | 100 | 0.16 | 0.16 | 0.19 | 0 | válida |
| PostgreSQL 18 | idempotencia | 100 | 2.39 | 2.72 | 2.80 | 0 | válida |
| Ixmati | consulta de estado | 100 | 2.47 | 3.47 | 3.90 | 0 | válida |
| SQLite directo | mixto | 100 | 0.17 | 0.97 | 1.07 | 0 | válida |
| PostgreSQL 18 | mixto | 100 | 2.65 | 6.65 | 9.04 | 0 | válida |
| Ixmati | mixto | 100 | 100 | 2.16 | 2003.02 | 0 | válida; mezcla 80/20 |

## Métricas y estado del pipeline

Los snapshots `ixmati-metrics-*.prom` muestran cache-aside y proyección con
hits, `outbox_size=0` después de que el pipeline drena y errores
`queue_full` durante los escalones de escritura. Durante 20/s se observó
`outbox_size=2` transitorio; después volvió a cero. En 100/s se observaron
respuestas `pending` y `queue_full`, coherentes con los `429` del cliente.

El endpoint utilizado no expuso una serie `consumer_queue_depth` en esta
ejecución; no se convirtió su ausencia en cero. Esa métrica debe añadirse o
exponerse en el runbook antes de usarla como criterio de alerta. El estado de
servicios se conserva en `ixmati-services-*.txt`; el manifiesto conserva SHA,
host, arquitectura, Podman y digest de PostgreSQL.

## Referencias oficiales de PostgreSQL

La documentación oficial de PostgreSQL 18 describe `pgbench` como una
herramienta para medir TPS, latencia, fallos y scripts personalizados. Su
salida ilustrativa contiene 896.967 TPS y 11.013 ms de latencia media; es un
ejemplo de formato, no una capacidad garantizada para este Debian ni es
comparable directamente con este workload:
<https://www.postgresql.org/docs/current/pgbench.html>.

El anuncio oficial de PostgreSQL 17 afirma que cargas de alta concurrencia
pueden obtener hasta 2× más throughput de escritura por mejoras en WAL. Es
una afirmación del proyecto PostgreSQL, no una medición de esta máquina ni de
Ixmati:
<https://www.postgresql.org/about/news/postgresql-17-released-2936/>.

## Límites y siguientes pasos

- `cold-first-pass` es primer pase lógico, no vacía necesariamente la page
  cache del kernel.
- Las mediciones directas usan conexiones persistentes por worker; no deben
  interpretarse como una API/pool completo de producción.
- Los escenarios `batch_size=100` de la corrida de 3 s tienen granularidad
  insuficiente en las tasas bajas y se conservan como diagnóstico, no como
  capacidad principal.
- El generador no mide pérdida de eventos durante un crash en esta suite; la
  prueba de crash y el conteo de duplicados siguen siendo `TASK-VAL-0033`.
- Pattern R mutable, alertas de `consumer_queue_depth` y la investigación del
  atasco MQTT siguen pendientes.

La captura suplementaria de estado en el SHA final `87c7e14` conserva actividad,
locks y WAL de PostgreSQL antes/después de una corrida corta. El snapshot
post-carga registró 18,918 commits acumulados, 30.1 MB de WAL y sólo
`AccessShareLock`/`ExclusiveLock` en la consulta de locks. La captura de
SQLite conserva el tamaño del archivo; el host no tenía el binario `sqlite3`
disponible para ejecutar las pragmas desde fuera del contenedor, por lo que no
se inventaron valores de checkpoint o `SQLITE_BUSY`.

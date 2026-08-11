# Evidencia de hardening de producción — 2026-08-11

## Identidad

- Repositorio: `rafex/Ixmati`
- SHA del árbol documentado: `85aabba`
- Host de validación: Debian amd64 mediante conexión Podman `debian-server`
- Contenedor: `ixmati-load-test`
- Artefacto: `dist/ixmati-0.1.0-linux-amd64.tar.gz`
- Arquitectura verificada: `ELF 64-bit LSB pie executable, x86-64`
- Configuración productiva restaurada al finalizar el escenario de crash; no
  quedó override de `IXMATI_TEST_MODE`.

## Gates locales

Pasaron:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `just test-integration`
- `just validate-config`
- `bash -n helpers/shell/crash_puback_window.sh`
- `git diff --check`
- `promtool check rules k8s/alerts.yaml` usando
  `prom/prometheus:v3.5.0`: 13 reglas válidas
- build release y `make dist dist-checksums dist-validate`

## Distribución Debian

`helpers/shell/test_installer_debian.sh` pasó con el artefacto Linux/amd64:

- instalación limpia: Mosquitto, cache-server, writer, API y projector activos;
- health check HTTP `200`;
- round-trip `POST /write` + `GET /read` desde cache;
- reinstalación idempotente conservando configuración;
- segundo round-trip con versión 2;
- `--uninstall --purge` eliminó servicios, binarios, datos y usuario.

Una ejecución anterior fue descartada porque `make build-release` había
empaquetado binarios Mach-O arm64 del Mac. La corrida válida se reconstruyó
con `make containers-builder && make containers-compile` y verificó ELF
x86-64 antes de instalar.

## Crash entre PUBACK y `published_at`

Comando:

```bash
CONTAINER_NAME=ixmati-load-test TEST_HOST=192.168.3.175 \
  OUT=/tmp/ixmati-puback-window-20260811-r2.tsv \
  helpers/shell/crash_puback_window.sh default 20
```

Resultado:

- barrera atómica alcanzada en `phase=puback_received_before_published_at`;
- manifiesto: `outbox_ids=[21]`;
- writer terminado con `SIGKILL` y reiniciado por systemd;
- `20/20` claves presentes en `_idempotency` con `applied_at`;
- `20/20` respuestas API `APPLIED` después de la recuperación;
- `20/20` `event_id` observados al menos una vez por el suscriptor MQTT;
- `outbox_pending=0` al finalizar;
- `1` duplicado observado, permitido por at-least-once.

Los artefactos completos de la ejecución fueron el TSV de resultados, el JSON
de barrera y el log del suscriptor MQTT en `/tmp` durante la corrida. La
prueba valida la ventana de confirmación; no convierte at-least-once en
exactly-once.

## Pattern R mutable

La prueba se ejecutó contra el compose multi-store en Debian amd64 con API en
`http://192.168.3.175:30000`, autenticación `ApiKey ix-default-key` y un
proyector reconstruido desde `85aabba`. Se usaron claves nuevas por ejecución.

Resultado observado para una relación `pedidos -> usuarios`:

| Caso | Resultado |
|---|---|
| creación de usuario + pedido | proyección inicial contenía ambos payloads |
| actualización de usuario v2 | la proyección cambió de `Antes-*` a `Despues-*` |
| eliminación de usuario v3 | la proyección desapareció (`found=false`) |
| publicación duplicada de evento v2 | no duplicó ni alteró el resultado |
| evento fuera de orden v1 después de v2 | el valor v1 obsoleto no regresó a la vista |

La propagación utilizó el índice inverso `ridx` y el payload del evento
secundario. La prueba confirma la consistencia eventual del proyector, no
consistencia síncrona entre stores.

La reconstrucción remota se validó usando los volúmenes Podman del stack y la
configuración copiada dentro de un contenedor temporal, evitando el bind mount
local del compose. `ixmati-reconciler` terminó con código 0, reconstruyó
`pedidos_con_usuario` con 4 entidades y `usuarios_materializados` con 1, sin
errores; la lectura posterior de una proyección conservó el valor v2.

El primer intento mediante el profile `reconciler` del compose falló antes de
iniciar el binario porque Podman remoto intentó resolver
`config/projections.toml` en el filesystem local del cliente. Ese fallo es una
limitación del harness, no del reconciler; el procedimiento reproducible es
copiar la configuración al host/contenedor remoto y montar los volúmenes por
directorio.

## Baseline durable y atribución del writer

Comando ejecutado contra el artefacto amd64 generado desde `85aabba`:

```bash
DURATION=10s CONCURRENCY=200 CONTAINER_NAME=ixmati-load-test \
  RESULT_DIR=/tmp/ixmati-staircase-85aabba \
  helpers/wrk/staircase.sh 192.168.3.175 30300 30301
```

El generador fue `python-rate-load`, con control de tasa y
`client_saturated_ticks=0` en los siete escalones. La tasa listada como
throughput es la tasa objetivo; la columna durable cuenta respuestas `200`.
No hubo respuestas `202` ni errores del generador.

| Objetivo/s | Durable/s | p50 ms | p90 ms | p99 ms | 429 | cola MQTT final | outbox tras drenado |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 20 | 19.1 | 71.1 | 77.1 | 135.5 | 9 | 0 | 0 |
| 40 | 39.1 | 74.3 | 108.2 | 136.7 | 9 | 0 | 0 |
| 60 | 58.7 | 102.8 | 135.4 | 146.8 | 13 | 0 | 0 |
| 80 | 78.5 | 105.6 | 141.7 | 156.9 | 15 | 0 | 0 |
| 100 | 96.7 | 109.4 | 150.2 | 184.7 | 33 | 0 | 0 |
| 150 | 143.9 | 171.7 | 243.2 | 312.8 | 61 | 0 | 0 |
| 200 | 194.3 | 229.7 | 328.2 | 442.2 | 57 | 0 | 0 |

La configuración productiva de 40/s queda por debajo del guardrail de 250 ms
de p99 y no mostró `202`, cola MQTT ni outbox pendiente después del drenado.
Desde 150/s la latencia y el backpressure aumentan; esos escalones son válidos
como capacidad bajo tasa controlada, pero no como capacidad sostenible
productiva sin aceptar el mayor porcentaje de `429`.

En el snapshot final del writer, acumulado durante la corrida, el ciclo de
batch promedió 22.3 ms: SQLite 13.7 ms, sincronización de cache 8.5 ms y
espera de la cola del hilo 0.05 ms. La suma explica aproximadamente 99% del
ciclo observado; la publicación MQTT continuó con conexión activa, sin
timeouts ni errores del event loop. Es atribución de la corrida completa, no
un p99 por segmento.

## Pendiente de esta evidencia

Esta evidencia ya cubre el baseline durable, crash PUBACK, Pattern R mutable y
reconciliación remota. Sigue pendiente reproducir o descartar el atasco MQTT
con el watchdog habilitado en un escenario de pérdida de progreso; mientras no
exista esa reproducción, `TASK-VAL-0035` no debe marcarse como resuelta por
inferencia.

# Runbook — Pruebas de carga contra el contenedor Debian remoto

Procedimiento usado en toda la investigación de DEC-0050 a DEC-0058 para
construir binarios, levantar un contenedor Debian real, cargarlo, y
diagnosticarlo. No es un tutorial genérico de Podman — es exactamente lo
que se repitió sesión tras sesión en este proyecto.

Este runbook aplica al contrato actual: `ack_mode=accepted` es un alias
compatible de `committed`, ambos esperan confirmación durable en SQLite, y
`200` sólo significa que `_idempotency` ya hizo commit. Si el commit no se
confirma dentro de `WRITE_COMMITTED_TIMEOUT_MS`, la respuesta correcta es
`202 PENDING`; el estado se consulta en `GET /writes/{store}/{key}`.

## 1. Antes de empezar — el malentendido de la IP

**`podman` es un comando que se escribe en el Mac, pero puede ejecutar en
otra máquina por completo.** Podman soporta conexiones remotas; si la
conexión activa apunta a un host distinto, *todo* `podman build/run/exec`
que se corre "en local" en realidad construye y ejecuta ahí — el contenedor
nunca toca el disco ni la red del Mac.

Confirmar la conexión activa **antes** de cualquier prueba:

```bash
podman system connection list
```

Si la columna `Default` marca `debian-server`
(`ssh://rafex@192.168.3.175:22/...`), cualquier contenedor que se levante
corre en `192.168.3.175`, no en `localhost`. Eso es lo que permite (y
obliga a) publicar puertos con `-p` y acceder por
`http://192.168.3.175:<puerto>` desde el Mac — `localhost:<puerto>` no
tiene nada escuchando ahí.

Si la conexión por defecto no es la esperada:

```bash
podman system connection default debian-server
```

Preflight obligatorio antes de construir o publicar puertos:

```bash
set -euo pipefail
TEST_HOST="${TEST_HOST:-192.168.3.175}"
EXPECTED_SHA="$(git rev-parse HEAD)"
test "$(git branch --show-current)" = "main"
test "$(git status --porcelain --untracked-files=no)" = ""
test "$(podman system connection list --format '{{.Name}} {{.Default}}' | awk '$2==\"true\"{print $1}')" = "debian-server"
podman version
curl -fsS "http://${TEST_HOST}:30300/health" || true
echo "testing_sha=${EXPECTED_SHA} host=${TEST_HOST}"
```

El `curl` puede fallar si todavía no existe el contenedor; en ese caso se
continúa con la instalación y se repite después de arrancar los servicios.
El SHA se conserva en todos los artefactos y debe coincidir con `origin/main`.

## 2. Rango de puertos: `30300-30399`

Esta es la convención acordada para publicar puertos de contenedores de
prueba — evita colisionar con otros servicios corriendo en el mismo host
compartido. (Nota: las corridas de DEC-0053 a DEC-0058 usaron puertos
sueltos fuera de este rango, ej. `30012`/`30013` — quedan documentadas así
en las decisiones ya cerradas por trazabilidad, pero **de acá en adelante
usar este rango**.)

Ejemplos de asignación dentro del rango:

| Uso | Puerto API (`:30000` interno) | Puerto métricas writer (`:9464` interno) |
|---|---|---|
| Corrida 1 | `30300` | `30301` |
| Corrida 2 | `30310` | `30311` |
| Corrida 3 | `30320` | `30321` |

## 3. Construir los binarios amd64

Repetir siempre que cambie código Rust. **No** hace falta repetirlo si solo
cambia `config/mosquitto/mosquitto.conf` o una unidad `systemd/*.service`
(esos se empaquetan tal cual desde el repo, no se compilan).

```bash
make ci-main               # gates Rust + build linux/amd64 + dist + checksums + validación
```

`containers-builder` corre en el host remoto (misma lógica de la sección
1) — puede tardar 1-3 minutos según cuánto haya cambiado.
Si se necesita separar los pasos para diagnosticar un fallo, se pueden usar
`make containers-builder`, `make containers-compile`, `make dist`,
`make dist-checksums` y `make dist-validate` en ese orden.

## 4. Levantar el contenedor de prueba

Dos modos distintos, no confundir:

**A. Contenedor efímero, para validar el instalador en sí** (7 pasos:
instalación limpia, roundtrip write/read, idempotencia, reinstalación,
desinstalación) — se autodestruye al terminar:

```bash
helpers/shell/test_installer_debian.sh
```

**B. Contenedor persistente, para dejarlo corriendo mientras se generan
varias corridas de carga** (lo que se usó en toda esta investigación):

```bash
podman rm -f ixmati-load-test 2>/dev/null

podman run -d --name ixmati-load-test --privileged \
    -p 30300:30000 -p 30301:9464 \
    -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
    localhost/ixmati-installer-test

sleep 3
podman exec ixmati-load-test systemctl is-system-running --wait \
  || podman exec ixmati-load-test systemctl is-system-running
# "degraded" es aceptable (unidades ajenas a Ixmati) — "running" también.

VERSION=$(cat VERSION)
TARBALL="dist/ixmati-${VERSION}-linux-amd64.tar.gz"
podman cp "$TARBALL" ixmati-load-test:/root/"$(basename "$TARBALL")"
podman exec ixmati-load-test bash -c "cd /root && tar xzf $(basename "$TARBALL")"
podman exec ixmati-load-test bash -c \
  "cd /root/ixmati-${VERSION}-linux-amd64 && ./install.sh"
```

Verificar salud antes de cargar:

```bash
curl -sS http://192.168.3.175:30300/health
```

## 5. Overrides de systemd usados en esta investigación

Siempre `systemctl daemon-reload` + `systemctl restart <unidad>` después de
escribir un override.

**Deshabilitar el throttle** (para medir el techo real del writer, no el
del rate-limiter — usado en DEC-0053/0055/0056/0057):

```bash
podman exec ixmati-load-test bash -c "mkdir -p /etc/systemd/system/ixmati-api.service.d && cat > /etc/systemd/system/ixmati-api.service.d/override.conf <<'EOF'
[Service]
Environment=MAX_WRITES_PER_WINDOW=1000000
Environment=THROTTLE_WINDOW_SECS=1
EOF
systemctl daemon-reload
systemctl restart ixmati-api"
```

**Exponer métricas del writer** (`METRICS_PORT`, no está activo por
defecto — ver `crates/ixmati-writer/src/metrics.rs`):

```bash
podman exec ixmati-load-test bash -c "mkdir -p /etc/systemd/system/ixmati-writer@.service.d && cat > /etc/systemd/system/ixmati-writer@.service.d/override.conf <<'EOF'
[Service]
Environment=METRICS_PORT=9464
EOF
systemctl daemon-reload
systemctl restart ixmati-writer@default"
```

Quitar cualquier override (volver al default de producción):

```bash
podman exec ixmati-load-test bash -c "rm -f /etc/systemd/system/ixmati-api.service.d/override.conf /etc/systemd/system/ixmati-writer@.service.d/override.conf /etc/systemd/system/ixmati-writer@default.service.d/90-crash-puback-window.conf /etc/systemd/system/ixmati-writer@.service.d/91-watchdog.conf && systemctl daemon-reload && systemctl restart ixmati-api ixmati-writer@default"
```

**Watchdog de progreso (sólo diagnóstico/recovery controlado):**

```bash
podman exec ixmati-load-test bash -c "mkdir -p /etc/systemd/system/ixmati-writer@.service.d && printf '%s\\n' '[Service]' 'Environment=MQTT_WATCHDOG_TIMEOUT_MS=30000' > /etc/systemd/system/ixmati-writer@.service.d/91-watchdog.conf && systemctl daemon-reload && systemctl restart ixmati-writer@default"
```

El valor `0` (default) lo desactiva. Sólo termina el writer si ya recibió
comandos y no logró un commit durante el intervalo; systemd lo reinicia. No se
debe usar para ocultar una causa MQTT: conservar primero CPU, `/proc`, journal,
cola `$SYS` y métricas. El override debe eliminarse al terminar.

## 6. Las 3 herramientas de carga del repo

| Script | `ack_mode` | Mide | Cuándo usarlo |
|---|---|---|---|
| `helpers/wrk/write.lua` | `committed` | Latencia end-to-end real; generador de stress de alta concurrencia | Sobrecarga controlada o pruebas de techo; cada `200` implica commit |
| `helpers/wrk/write_committed.lua` | `committed` | Latencia end-to-end real con concurrencia baja | Medir SLOs sin que el generador sea el cuello de botella |
| `helpers/wrk/staircase.sh` | `committed` | Latencia, estados HTTP y métricas por escalón | Capacidad sostenible; usa `wrk2` o `helpers/python/rate_load.py` |

Ejemplos (con el rango de puertos de la sección 2):

```bash
# Stress de alta concurrencia (committed; no confundir con una tasa exacta)
wrk -t4 -c50 -d90s -s helpers/wrk/write.lua http://192.168.3.175:30300/write

# Latencia real (committed) — usar con el throttle en su valor de producción
wrk -t2 -c10 -d30s --timeout 5s -s helpers/wrk/write_committed.lua http://192.168.3.175:30300/write

# Escalera automática — usa wrk2 (-R) o el fallback estándar de Python con
# tasa global fija. Si sólo existe wrk, los escalones altos quedan inconclusos.
helpers/wrk/staircase.sh 192.168.3.175 30300 30301
```

`staircase.sh` necesita el contenedor ya instalado y `METRICS_PORT` activo
en el writer (sección 5). Hace los overrides de `MAX_WRITES_PER_WINDOW`
por escalón automáticamente y registra si el generador fue `wrk2`,
`python-rate-load` o `wrk`. El fallback de Python genera una idempotency key
única por request y limita la concurrencia, por lo que sus tasas son válidas
para comparar capacidad; sólo el fallback `wrk` queda marcado como
inconcluso en los escalones altos.
Cada corrida debe conservar la salida completa, incluyendo p50/p90/p99,
conteos HTTP `200`/`202`/`429`, `outbox_size`, `consumer_queue_depth`,
`last_batch_commit_unix_seconds`, errores de cache y mensajes del broker.

Para verificar durabilidad durante un crash:

```bash
CONTAINER_NAME=ixmati-load-test TEST_HOST=192.168.3.175 \
  OUT="/tmp/ixmati-kill9-$(date +%Y%m%dT%H%M%S).tsv" \
  helpers/shell/kill9_writer.sh default 100
```

El script publica claves de idempotencia conocidas, fuerza `SIGKILL`,
reinicia `ixmati-writer@default` con systemd, consulta cada clave en la API
y SQLite, y compara los `event_id` del outbox con un suscriptor MQTT. Los
duplicados son evidencia de at-least-once, no una pérdida; cualquier clave o
evento ausente es fallo.

Para forzar la ventana exacta `PUBACK → published_at`, usar el failpoint
exclusivo de pruebas. El script instala `IXMATI_TEST_MODE=1` y una barrera
atómica, espera el manifiesto con los `outbox_ids` que recibieron PUBACK, mata
el writer con `SIGKILL`, elimina el override antes del restart y verifica
`_idempotency`, estado `APPLIED`, `_outbox`, `published_at` y eventos MQTT:

```bash
CONTAINER_NAME=ixmati-load-test TEST_HOST=192.168.3.175 \
  OUT="/tmp/ixmati-puback-window-$(date +%Y%m%dT%H%M%S).tsv" \
  helpers/shell/crash_puback_window.sh default 20
```

Si la barrera no aparece, el resultado es inconcluso; no se sustituye por un
`sleep`. Los duplicados observados se cuantifican y son válidos por
at-least-once; un evento confirmado ausente es fallo.

## 7. Protocolo de diagnóstico (atascos silenciosos)

Usado en DEC-0055/0056/0057 para distinguir "el writer está lento" de "el
writer dejó de avanzar" sin asumir nada por la ausencia de logs nuevos.

**CPU real del proceso** (2 muestras separadas ~3s — el `%CPU` que reporta
`ps` es un promedio acumulado desde el arranque, no sirve para esto):

```bash
podman exec ixmati-load-test bash -c '
pid=$(pgrep -x ixmati-writer)
read_ticks() { awk "{print \$14+\$15}" /proc/$pid/stat; }
t1=$(read_ticks); sleep 3; t2=$(read_ticks)
echo "ticks_delta=$((t2-t1)) sobre 3s (100 ticks/s)"
'
```

**Cola de Mosquitto** (2 muestras separadas ~3s):

```bash
podman exec ixmati-load-test bash -c "
timeout 2 mosquitto_sub -t '\$SYS/broker/messages/stored' -C 1
sleep 3
timeout 2 mosquitto_sub -t '\$SYS/broker/messages/stored' -C 1
"
```

**Estado de todos los hilos del proceso**:

```bash
podman exec ixmati-load-test bash -c '
pid=$(pgrep -x ixmati-writer)
for t in /proc/$pid/task/*/; do
  tid=$(basename "$t")
  comm=$(cat "$t/comm" 2>/dev/null)
  state=$(awk "/^State:/{print \$2}" "$t/status")
  echo "tid=$tid comm=$comm state=$state"
done
'
```

Si CPU ≈ 0 y la cola de Mosquitto no baja entre las 2 muestras, es un
atasco real, no lentitud — confirmar además que no hay líneas de
error/panic (`journalctl -u ixmati-writer@default | grep -i "error\|panic"`)
antes de concluir cuál es la causa.

Con el watchdog activo, un atasco reproducible debe mostrar en el journal el
mensaje `pending commands but no durable progress`, salir con código 42 y ser
reiniciado por systemd. Comparar los reinicios con
`mqtt_eventloop_errors_total`, `mqtt_ack_failures_total`,
`outbox_puback_timeouts_total` y `last_batch_commit_unix_seconds`; el watchdog
es recuperación, no diagnóstico de causa raíz.

## 8. Pattern R mutable y reconciliación

La primera escritura de un Pattern R registra un índice interno bajo
`ridx:<projection>:<store>:<entity>:<key>`. Para validar una relación mutable:

1. crear la entidad referenciada y la entidad primaria;
2. esperar `projector_events_processed_total` y leer la proyección inicial;
3. actualizar sólo la entidad referenciada con una versión mayor;
4. esperar que `projector_last_event_unix_seconds` avance y confirmar que la
   proyección contiene el valor nuevo;
5. repetir con duplicado MQTT, evento fuera de orden y eliminación;
6. reiniciar o limpiar cache, ejecutar `ixmati-reconciler` y confirmar que el
   índice y la vista vuelven a aparecer.

La propagación es eventual, con fan-out máximo de 100 dependientes. Antes de
procesar el evento la vista puede conservar el snapshot anterior; después de
procesarlo correctamente debe contener el valor nuevo. La pérdida de cache no
se considera corregida hasta ejecutar reconciler y guardar sus logs.

## 9. Comparativa SQLite directo / Ixmati / PostgreSQL

La comparación completa se ejecuta en el mismo Debian amd64 y nunca mantiene
los motores bajo prueba activos simultáneamente:

```bash
BENCH_DURATION=30 BENCH_WARMUP=15 BENCH_REPETITIONS=3 \
  just benchmark-db
```

`benchmarks/run_suite.sh` levanta PostgreSQL 18 desde la imagen oficial,
prepara 1,000 usuarios y 10,000 pedidos por defecto (configurables mediante
`BENCH_USERS` y `BENCH_ORDERS`), ejecuta lecturas puntuales,
lecturas con relación, inserciones, actualizaciones, idempotencia y carga
mixta. También ejecuta el camino HTTP de Ixmati y guarda cada JSON, log y
manifiesto bajo `spec-native/evidence/raw/`.

El baseline SQLite usa exactamente `WAL`, `synchronous=NORMAL` y
`busy_timeout=5000`. El baseline PostgreSQL usa `synchronous_commit=on`.
Las corridas `cold-first-pass` son primeras lecturas sin warmup sobre una base
recién preparada; no pretenden vaciar la page cache del kernel. Las corridas
`warm` tienen 15s de warmup.

Una corrida sólo es válida si `client_saturated_ticks` vale cero. La tabla
final debe separar los resultados directos de base de datos del camino
completo de Ixmati, porque Ixmati incluye HTTP, MQTT, batching, idempotencia,
outbox, cache y proyecciones.

Las referencias oficiales de PostgreSQL se documentan por separado en la
evidencia: [`pgbench`](https://www.postgresql.org/docs/current/pgbench.html)
define el formato de TPS/latencia y el anuncio oficial de PostgreSQL 17
reporta mejoras de hasta 2x en throughput de escritura bajo alta concurrencia
([fuente](https://www.postgresql.org/about/news/postgresql-17-released-2936/)).

## 10. Limpieza

Siempre al terminar — un contenedor de prueba olvidado sigue corriendo en
la máquina remota compartida, no en el Mac:

```bash
podman exec ixmati-load-test bash -c "rm -f /etc/systemd/system/ixmati-api.service.d/override.conf /etc/systemd/system/ixmati-writer@.service.d/override.conf && systemctl daemon-reload && systemctl restart ixmati-api ixmati-writer@default"
podman rm -f ixmati-load-test
```

Antes de terminar, guardar como evidencia el SHA probado, `podman version`,
la conexión activa, configuración de puertos, salida de la escalera, logs de
systemd, snapshots de `/metrics`, manifiestos de crash y consultas SQLite.
Los resultados deben quedar bajo una carpeta identificada por timestamp y
SHA, por ejemplo `dist/load-results/20260810T230000Z-<sha>/`; no se deben
reportar números de una corrida anterior como si fueran del SHA actual.

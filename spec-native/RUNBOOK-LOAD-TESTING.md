# Runbook — Pruebas de carga contra el contenedor Debian remoto

Procedimiento usado en toda la investigación de DEC-0050 a DEC-0058 para
construir binarios, levantar un contenedor Debian real, cargarlo, y
diagnosticarlo. No es un tutorial genérico de Podman — es exactamente lo
que se repitió sesión tras sesión en este proyecto.

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
make containers-builder    # compila el builder image en el host remoto
make containers-compile    # extrae los binarios linux/amd64 a target/release/
make dist                  # empaqueta dist/ixmati-<VERSION>-linux-amd64.tar.gz
make dist-checksums
```

`containers-builder` corre en el host remoto (misma lógica de la sección
1) — puede tardar 1-3 minutos según cuánto haya cambiado.

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
podman exec ixmati-load-test bash -c "rm -f /etc/systemd/system/ixmati-api.service.d/override.conf && systemctl daemon-reload && systemctl restart ixmati-api"
```

## 6. Las 3 herramientas de carga del repo

| Script | `ack_mode` | Mide | Cuándo usarlo |
|---|---|---|---|
| `helpers/wrk/write.lua` | `accepted` | Solo aceptación por la API, no persistencia | Generar backlog/sobrecarga a propósito (ver DEC-0055/0056/0057) |
| `helpers/wrk/write_committed.lua` | `committed` | Latencia end-to-end real (hasta commit confirmado) | Medir SLOs reales, nunca usar `write.lua` para esto |
| `helpers/wrk/staircase.sh` | `committed` | Latencia bajo una escalera de tasas de entrada controladas | Encontrar el throughput sostenible, no un número de commits/s aislado |

Ejemplos (con el rango de puertos de la sección 2):

```bash
# Saturar a propósito (accepted, sin límite de tasa)
wrk -t4 -c50 -d90s -s helpers/wrk/write.lua http://192.168.3.175:30300/write

# Latencia real (committed) — usar con el throttle en su valor de producción
wrk -t2 -c10 -d30s --timeout 5s -s helpers/wrk/write_committed.lua http://192.168.3.175:30300/write

# Escalera automática — usa wrk2 (-R, tasa fija real) si está instalado,
# si no cae a wrk con concurrencia fija (menos preciso en escalones altos,
# ver DEC-0058)
helpers/wrk/staircase.sh 192.168.3.175 30300 30301
```

`staircase.sh` necesita el contenedor ya instalado y `METRICS_PORT` activo
en el writer (sección 5) — hace los overrides de `MAX_WRITES_PER_WINDOW`
por escalón automáticamente, no hace falta repetir la sección 5 para eso.

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

## 8. Limpieza

Siempre al terminar — un contenedor de prueba olvidado sigue corriendo en
la máquina remota compartida, no en el Mac:

```bash
podman rm -f ixmati-load-test
```

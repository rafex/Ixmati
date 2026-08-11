# Evidencia de hardening de producción — 2026-08-11

## Identidad

- Repositorio: `rafex/Ixmati`
- SHA del árbol documentado: `02e02d450ffe52e177c54bf7b226c5ca44638021`
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

## Pendiente de esta evidencia

Esta ejecución no cierra por sí sola el baseline de latencia a 40/s, la
reproducción del atasco MQTT ni la matriz multi-store de Pattern R. Esas
corridas deben registrar sus propios snapshots de métricas, logs, estado de
servicios y SHA antes de marcar `TASK-VAL-0025`, `TASK-VAL-0035` o
`TASK-VAL-0036` como `done`.

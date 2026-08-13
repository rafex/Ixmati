# Estado verificable de producción

Fecha de corte: 2026-08-13 UTC
SHA publicado: `766b411a6511d11e0b973afdfe363e5862df68a2`

Este documento separa lo demostrado de los gates abiertos. Las evidencias
crudas de carga permanecen fuera de Git en `spec-native/evidence/raw/`; sólo
los resultados resumidos se versionan.

## Cambios revisados y publicados

- `8451d05`: publicador de outbox impulsado por PUBACK, sin esperar cinco
  segundos por un lote completo; publicaciones fallidas se retiran del
  tracker, el marcado de `published_at` vuelve al hilo único de SQLite y las
  colas de PUBACK quedan acotadas.
- `766b411`: arnés y documentación de réplica/restore Litestream mediante una
  ruta local `file://`.

`main` y `origin/main` apuntan al mismo SHA `766b411`.

## Resultados medidos

| Escenario | Resultado | Clasificación |
|---|---|---|
| 10 escrituras/s, 1 hora, SHA `6c38eb8` | 36,001/36,001 HTTP 200; p50 121.806 ms; p95 212.421 ms; p99 212.856 ms; 0 timeouts; outbox pendiente 0; `integrity_check=ok`; 0 reinicios | Perfil recomendado sostenible |
| 150 escrituras/s, 300 s, SHA `766b411` | 45,000/45,000 a tasa controlada; 0 ticks de saturación del cliente; 42,265 HTTP 200 (93.92%); 2,735 HTTP 202 (6.08%); 0 HTTP 429; p50 65.854 ms; p90 893.539 ms; p99 2,076.592 ms; outbox final 0; `integrity_check=ok` | Diagnóstico; no productivo |
| 200 escrituras/s | Corridas previas saturaron el generador y no permiten inferir el techo del servidor | Inconcluso |

En la corrida actual de 150/s, los 45,000 comandos terminaron publicados y el
outbox quedó drenado. Los `202` significan que la API no pudo confirmar
`_idempotency` dentro de los 2,000 ms; no son una confirmación durable y no se
cuentan como éxito del perfil productivo.

## Validaciones funcionales

- Crash entre PUBACK y `published_at`: 20/20 idempotencias recuperadas,
  20/20 eventos observados, outbox drenado y un duplicado permitido por
  at-least-once.
- Pattern R mutable: actualización, eliminación, duplicado y evento fuera de
  orden verificados; reconciler reconstruyó cache y proyecciones.
- Stores: rename, merge con tombstone/LWW, split reproducible, checksums,
  backup local y reconciler verificados. El cutover de routing/topics y
  rollback con tráfico siguen pendientes.
- Instalador Debian amd64: instalación, seis servicios activos, health 200,
  write/read, reinstalación idempotente, restore local y purge verificados.
- Litestream local: `file:///backup/test.db` → restore íntegro, con
  idempotencia y outbox preservados.
- Protobuf/gRPC y REST/JSON: unary, REST binario, replay/live local,
  autenticación, límites de recursos y benchmark corto ejecutados. Ningún
  protocolo aumentó la capacidad durable del writer.

## Gates ejecutados

Pasaron en el ciclo actual: `cargo fmt --all -- --check`, Clippy estricto,
`cargo test --workspace`, `just test-integration`, `just validate-config`,
`make proto`, `bash -n` de scripts, `git diff --check`, build/distribución
Linux amd64 (`make dist dist-checksums dist-validate`), validación de Compose,
instalador Debian y `just litestream-local`.

`mdbook` no está instalado localmente, por lo que el build de documentación
mdBook no queda certificado por este entorno.

## Prueba interrumpida

Se inició una nueva corrida de una hora a 10/s sobre un contenedor Debian
efímero, con generador interno separado, pero fue interrumpida antes de
completar el régimen. El contenedor y el generador fueron eliminados; esa
ejecución no se clasifica como evidencia de una hora.

## Estado final

Ixmati es un producto viable beta con un perfil productivo demostrado de 10
escrituras durables/s por store, durabilidad SQLite/outbox, cache/proyecciones,
instalador Debian y una interfaz REST/gRPC funcional. No es correcto anunciar
150–200/s como capacidad productiva ni declarar alta disponibilidad completa.

Los gates que faltan para ampliar la declaración son: soak válido de 150/200,
restore remoto y RPO/RTO, cutover/rollback de stores, TLS/reverse proxy,
pruebas remotas de retención/reconexión de streams y completar el build mdBook.

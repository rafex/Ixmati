# Evidencia — réplica y restore S3-compatible de Litestream

## Alcance

Esta ejecución verifica que el camino de Litestream `0.5.16` usado por Ixmati
puede replicar una base SQLite WAL a un endpoint S3-compatible y restaurarla en
otro volumen. Es un smoke test reproducible con recursos Podman desechables;
no modifica los stores del host.

## Identidad de la ejecución

- Commit del arnés y configuración: `df8db9a`
- Script: `helpers/shell/test_litestream_s3.sh`
- Runtime: Podman
- Imagen MinIO: `quay.io/minio/minio@sha256:a1a8bd4ac40ad7881a245bab97323e18f971e4d4cba2c2007ec1bedd21cbaba2`
- Imagen `mc`: `quay.io/minio/mc@sha256:eb4ea9884b77704230e2423e9004d2fa738dc272876b9cc41a297d29443b8780`
- Imagen Debian usada para SQLite/verificación:
  `docker.io/library/debian@sha256:38a76d01668772e381ad2826d876627c89e7133e2f8a0f5d567306798b0f2a16`
- Imagen Litestream: `localhost/ixmati-litestream:local`, construida desde
  `containers/litestream/Containerfile` con Litestream `0.5.16`

## Comando

```bash
helpers/shell/test_litestream_s3.sh
```

El arnés crea una base con WAL, una fila de payload, una fila de
`_idempotency` y una fila de `_outbox`; inicia MinIO, replica mediante el
watcher de directorio de Litestream, comprueba objetos LTX y restaura la base
en un volumen separado.

## Resultado observado

La ejecución terminó correctamente con:

```text
s3_restore=ok; integrity=ok; idempotency=1
[litestream-s3-e2e] OK: replica S3-compatible y restore verificados
```

El listado de MinIO mostró objetos LTX bajo `ixmati/test.db/`, incluyendo
segmentos iniciales y posteriores a la creación de la base. La base restaurada
pasó `PRAGMA integrity_check`, conservó el payload y conservó la fila de
idempotencia.

## Limitaciones

Esta evidencia no demuestra todavía:

- restore desde el bucket S3 remoto de producción;
- credenciales, endpoint, políticas o TLS del proveedor real;
- dos destinos independientes;
- RPO o RTO medidos ante pérdida del host;
- restore destructivo del servicio completo;
- cutover/rollback de routing, topics o unidades systemd;
- capacidad sostenible de 150/s o 200/s.

Por tanto, `TASK-WRITE-0014` permanece abierta para el despliegue remoto y
las métricas operativas de recuperación. El smoke test reduce el riesgo de
configuración y transporte, pero no convierte la réplica local o MinIO en
evidencia de alta disponibilidad.

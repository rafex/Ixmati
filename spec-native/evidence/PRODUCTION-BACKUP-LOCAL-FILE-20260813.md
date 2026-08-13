# Evidencia — Litestream local con `file://`

- Fecha UTC: 2026-08-13
- SHA probado: `8451d05d0abe6a71f9d391b9a84644a40943cc99`
- Entorno: Podman remoto, Debian amd64
- Imagen Litestream: `localhost/ixmati-litestream:local`, Litestream `0.5.16`
- Imagen Debian: `docker.io/library/debian@sha256:38a76d01668772e381ad2826d876627c89e7133e2f8a0f5d567306798b0f2a16`
- Arnés: `helpers/shell/test_litestream_local.sh`

## Comando

```bash
helpers/shell/test_litestream_local.sh
```

El arnés crea tres volúmenes Podman desechables: base SQLite, destino de
backup montado y volumen de restore. Ejecuta el flujo directo:

```bash
litestream replicate /data/test.db file:///backup/test.db
litestream restore -o /restore/restored.db file:///backup/test.db
```

La ruta conserva el nombre del archivo dentro del directorio montado. Esto es
necesario para que el CLI directo pueda resolver la réplica durante `restore`;
el servicio nativo usa el mismo layout por store mediante su configuración.

## Resultado

La ejecución pasó con:

```text
local_file_uri_restore=ok; integrity=ok; idempotency=1; outbox=1
[litestream-local] OK: réplica file:// y restore verificados
```

Se verificó una escritura posterior a la réplica inicial, `PRAGMA
integrity_check`, una fila en `_idempotency` y una fila en `_outbox`. El
destino local es suficiente para el caso normal de un disco o NAS montado y no
requiere S3.

## Límites

Esta prueba valida transporte y restore local en volúmenes separados. No
demuestra tolerancia a pérdida del host, segundo destino, RPO/RTO medidos ni
restore remoto. Esos gates siguen separados de la ruta local.

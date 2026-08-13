# Evidencia — instalación nativa y restore local de Litestream

- Fecha UTC: 2026-08-13
- Commit probado: `3b30309`
- Artefacto: `ixmati-0.1.0-linux-amd64.tar.gz`
- Entorno: `debian:trixie-slim`, amd64, systemd como PID 1 en Podman
  privilegiado
- Arnés: `helpers/shell/test_installer_debian.sh`
- Versión Litestream: `0.5.16`
- SHA256 del binario Litestream verificado por el instalador:
  `9e29112380a942e4a62ee07773684396cb8b308dc4d67e130bef41f75e937f0a`

## Resultado

La ejecución pasó completa:

1. `make dist`, `make dist-checksums` y `make dist-validate` construyeron el
   artefacto desde el builder Linux/amd64. Cada binario fue validado como ELF
   64-bit x86-64; no se reutilizó el Mach-O/arm64 del host.
2. La instalación limpia dejó activos Mosquitto, cache-server, writer, API,
   projector e `ixmati-litestream-file`.
3. El round-trip REST escribió y leyó una entidad con estado `APPLIED` y la
   lectura provino de cache.
4. Litestream creó `/var/lib/ixmati/backups/default.db` y el restore:

   ```bash
   /usr/local/lib/ixmati/litestream restore \
     -o /tmp/default-restored.db \
     file:///var/lib/ixmati/backups/default.db
   ```

   terminó con `PRAGMA integrity_check = ok` y al menos un payload.
5. La reinstalación conservó configuración, mantuvo los servicios activos y
   pasó un segundo round-trip.
6. `install.sh --uninstall --purge` detuvo y eliminó unidades, binarios,
   configuración, datos y usuario `ixmati`.

La prueba de ciclo de vida `helpers/shell/test_store_migration_e2e.sh`, contra
el mismo commit y artefacto, también pasó rename, backup offline, merge con
`deduplicated_idempotency=1`, tombstone ganador, split reproducible en tres
destinos, reconciler, cache, integridad y outbox drenado.

## Límites

Esta evidencia demuestra la operación nativa y la restauración local. No
demuestra todavía:

- replicación efectiva a un bucket S3 o segundo VPS;
- restore destructivo desde un destino remoto;
- RPO menor a 5 segundos o RTO menor a 60 segundos;
- cutover de routing/topics y rollback con tráfico real;
- capacidad sostenible de 150/s o 200/s.

El instalador no habilita la unidad S3 si no existe
`IXMATI_LITESTREAM_S3_BUCKET`. Por ello `TASK-WRITE-0014` permanece abierta
para el destino remoto y las métricas RPO/RTO.

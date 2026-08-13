# Evidencia — ciclo de vida offline de stores

- Fecha UTC: 2026-08-13
- SHA de los binarios probados: `0d59b813aad7a4b60581649c30f41629f488729b`
- Entorno: contenedor Debian trixie amd64 con systemd privilegiado
- Arnés: `helpers/shell/test_store_migration_e2e.sh`
- Publicación: no se modificó el host; el contenedor fue eliminado por el
  `trap` de limpieza

## Resultado

La ejecución completa pasó:

1. Instalación nativa desde `make dist` y health check en `30000`.
2. Escritura durable con `ack_mode=committed`, drenado del outbox y parada de
   API, writer y projector antes de migrar.
3. `rename default → orders`, con `plan`, `execute`, `verify`, checksum e
   integridad SQLite.
4. Restauración del backup offline generado por `execute`:
   `PRAGMA integrity_check=ok`, una fila de payload y una fila de idempotencia.
5. `merge orders_a + orders_b → merged` con:
   - 2 payloads vivos y 1 tombstone;
   - 1 conflicto LWW contabilizado;
   - tombstone de versión 3 ganador sobre payload de versión 2;
   - 3 filas de idempotencia y 4 eventos outbox publicados.
6. `split merged → split_a, split_b, split_c`, con integridad y outbox drenado
   en cada destino.
7. Repetición del split sobre una copia idéntica: los tres checksums fueron
   idénticos a la primera ejecución.
8. Reconciler sobre el destino rename y lectura de la vista materializada desde
   cache:

```json
{"found":true,"key":"k1","payload":{"hello":"migration","key":"k1"},"projection":"orders_materialized"}
```

## Limitaciones

Esta evidencia valida el motor offline, sus invariantes SQLite, el backup
local previo a la migración y la reconstrucción de cache/proyecciones. No
valida todavía:

- restauración real desde un destino Litestream remoto ni RPO/RTO medidos;
- rechazo de topics antiguos y cutover de routing/configuración multi-store;
- rollback después de aceptar escrituras en los destinos;
- soak de capacidad de 150/s y 200/s durante una hora.

Por tanto `TASK-STORE-0005` queda parcialmente cerrada en la parte de
merge/split y backup local, pero continúa abierta para backup remoto, cutover
operativo y topics estrictos.

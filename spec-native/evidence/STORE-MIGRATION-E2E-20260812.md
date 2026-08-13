# Evidencia — cutover de migración y reconstrucción E2E

- Fecha UTC: 2026-08-12
- SHA publicado probado: `e30492ee7c97c2935342a86879f62ea9b8bb177d`
- Entorno: contenedor Debian trixie con systemd privilegiado
- Arnés: `helpers/shell/test_store_migration_e2e.sh`

## Resultado

La ejecución pasó de extremo a extremo:

1. Instaló el artefacto amd64 y dejó activos Mosquitto, cache-server, API,
   writer y projector.
2. Escribió una entidad durable con `ack_mode=committed` y esperó outbox
   pendiente igual a cero.
3. Detuvo API, projector y writer antes de ejecutar la migración.
4. Ejecutó `plan`, `execute` y `verify` para `default → orders`.
5. Ejecutó `ixmati-reconciler` contra `orders.db` y un manifiesto de
   proyección materializada temporal.
6. Reinició API y leyó la proyección por HTTP/cache:

```json
{"found":true,"key":"k1","payload":{"hello":"migration","key":"k1"},"projection":"orders_materialized"}
```

7. `PRAGMA integrity_check` devolvió `ok` y el outbox del destino quedó en
   cero eventos pendientes.

## Alcance aún abierto

Esta ejecución histórica validó rename, cutover controlado, reconciler y cache.
La ejecución ampliada de 2026-08-13 está documentada en
`STORE-MIGRATION-E2E-20260813.md`.

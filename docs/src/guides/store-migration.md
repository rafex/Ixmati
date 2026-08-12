# Migración de stores

Las operaciones de rename, merge y split son offline. Se usa el binario
`ixmati-store-migrate` sobre archivos SQLite quiescidos y después se ejecuta
el reconciler para reconstruir cache y vistas.

Consulta el procedimiento completo en
[`spec-native/RUNBOOK-STORE-MIGRATION.md`](../../../spec-native/RUNBOOK-STORE-MIGRATION.md).

El merge utiliza LWW determinista por versión, timestamp y nombre de origen.
Las eliminaciones se conservan como tombstones. El split usa
`sha256-key-v1`, por lo que la misma clave siempre cae en el mismo destino.

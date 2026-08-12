# Runbook — Migración offline de stores

Este procedimiento cubre `rename`, `merge` y `split` con
`ixmati-store-migrate`. La herramienta opera únicamente sobre SQLite y no
detiene servicios ni cambia routing. Toda migración requiere una ventana de
mantenimiento.

## Precondiciones

- Confirmar el SHA y el binario publicado.
- Detener API, writers, projectors, reconciler, Litestream y productores.
- Esperar a que `_outbox WHERE published_at IS NULL` sea cero.
- Ejecutar backup verificable por cada store.
- Tener espacio para backups y destinos temporales.
- Usar `quiesced = true` en el manifiesto.
- No tener ningún archivo destino existente.

La herramienta aborta ante `quick_check`, outbox pendiente, destino existente,
colisión de idempotencia divergente o información histórica insuficiente. No
existe `--force` en la versión inicial.

## Planificar y ejecutar

El ejemplo está en [`benchmarks/migration.example.toml`](../benchmarks/migration.example.toml).

```bash
ixmati-store-migrate plan --manifest migration.toml
ixmati-store-migrate verify --manifest migration.toml
ixmati-store-migrate execute --manifest migration.toml
```

`execute` crea backups en `evidence_dir`, escribe destinos temporales en el
mismo filesystem, valida integridad y los publica con rename atómico. Un fallo
deja el origen intacto y no publica destinos parciales.

## Rename

`rename` reescribe el identificador del store en payload, idempotencia y
outbox. Se conservan versiones, eventos e historial publicado. Después del
cutover se actualizan `stores.toml`, rutas SQLite, unidades systemd,
configuración Litestream y topics. El nombre anterior se retira de forma
estricta: no hay alias ni doble publicación.

## Merge

Para el mismo `(entity, key)`, gana de forma determinista:

1. mayor `version`;
2. mayor timestamp (`updated_at` o `deleted_at`);
3. nombre de store origen lexicográficamente menor.

El reporte conserva el número de conflictos. Una eliminación se representa
como tombstone y puede ganar frente a un payload vivo. Las colisiones de
idempotencia con digest idéntico se deduplican; una colisión divergente aborta.

El historial publicado del outbox se conserva, pero no se reproduce
automáticamente. Las proyecciones se reconstruyen con reconciler.

## Split

Para un store origen y una lista ordenada de destinos:

```text
bucket = uint64_be(SHA-256(key UTF-8)[0..8]) mod número_de_destinos
```

Payloads, tombstones, idempotencia y outbox usan el mismo bucket. Las filas sin
clave de entidad se rechazan. El algoritmo `sha256-key-v1` queda fijado en el
manifiesto para que una repetición produzca los mismos checksums.

## Cutover y rollback

1. detener servicios y productores;
2. drenar y verificar outbox;
3. ejecutar `plan`, revisar conflictos y ejecutar `execute`;
4. cambiar configuración y retirar topics antiguos;
5. invalidar cache;
6. ejecutar `ixmati-reconciler` sobre los destinos;
7. arrancar broker, writers, projectors y API;
8. probar escritura, lectura, cache, proyecciones y salud;
9. conservar evidencia y backups.

El rollback directo sólo es válido antes de aceptar nuevas escrituras en los
destinos. Después de eso se necesita otra migración offline o restauración
explícita con posible pérdida según el backup elegido. No se promete una
transacción distribuida entre stores.

## Verificación posterior

```bash
sqlite3 /var/lib/ixmati/stores/<store>.db 'PRAGMA integrity_check;'
sqlite3 /var/lib/ixmati/stores/<store>.db \
  "SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL;"
IXMATI_STORE_PATHS='orders=/var/lib/ixmati/stores/orders.db' \
  cargo run -p ixmati-reconciler
curl -fsS http://127.0.0.1:8080/health
```

Debe verificarse que el nombre anterior ya no acepte escrituras, que no haya
eventos confirmados ausentes y que las vistas/cache converjan después de la
reproyección.

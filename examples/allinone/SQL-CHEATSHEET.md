# Ixmati SQL Cheatsheet

Referencia rápida para operadores y desarrolladores que necesitan inspeccionar
o consultar directamente la base de datos SQLite de Ixmati.

## Acceso

```bash
# Acceso interactivo
podman exec -it ixmati-allinone sqlite3 /var/lib/ixmati/stores/default.db

# Consulta única
podman exec ixmati-allinone sqlite3 /var/lib/ixmati/stores/default.db "SELECT ..."

# Modo columnas (más legible)
podman exec ixmati-allinone sqlite3 -column -header /var/lib/ixmati/stores/default.db "SELECT ..."
```

## Schema (store `default`)

```sql
-- Tabla principal de datos (1 por store)
CREATE TABLE payload_default (
    entity     TEXT NOT NULL,
    key        TEXT NOT NULL,
    version    INTEGER NOT NULL,
    payload    BLOB NOT NULL,         -- JSON serializado
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (entity, key)
);

-- Tabla de idempotencia (dedup de comandos)
CREATE TABLE _idempotency (
    idempotency_key TEXT NOT NULL,
    store           TEXT NOT NULL,
    entity          TEXT NOT NULL,
    key             TEXT NOT NULL,
    version         INTEGER NOT NULL,
    applied_at      TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (store, idempotency_key)
);

-- Tabla de outbox de eventos
CREATE TABLE _outbox (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id      TEXT NOT NULL,
    event_type    TEXT NOT NULL,       -- ej: "pedido.creado", "pedido.eliminado"
    store         TEXT NOT NULL,
    entity        TEXT NOT NULL,
    key           TEXT NOT NULL,
    version       INTEGER NOT NULL,
    occurred_at   TEXT NOT NULL,
    payload       BLOB NOT NULL,
    published_at  TEXT                 -- NULL = no publicado, datetime = publicado
);

CREATE INDEX idx_outbox_published ON _outbox(published_at);
CREATE INDEX idx_outbox_store ON _outbox(store, published_at);
```

## Queries útiles

### Inspección general

```sql
-- Contar registros por entidad
SELECT entity, COUNT(*) as cnt, MAX(version) as max_ver
FROM payload_default
GROUP BY entity
ORDER BY cnt DESC;

-- Últimos 10 registros modificados
SELECT entity, key, version, updated_at
FROM payload_default
ORDER BY updated_at DESC
LIMIT 10;

-- Buscar por entidad
SELECT entity, key, version,
       json_extract(payload, '$') as data
FROM payload_default
WHERE entity = 'pedido'
ORDER BY key;
```

### JSON extraction

```sql
-- Extraer campos de JSON en payload
SELECT key,
       json_extract(payload, '$.total') as total,
       json_extract(payload, '$.estado') as estado,
       json_extract(payload, '$.usuario_id') as usuario_id
FROM payload_default
WHERE entity = 'pedido';

-- Filtrar por campo JSON
SELECT entity, key, json_extract(payload, '$.nombre') as nombre
FROM payload_default
WHERE entity = 'usuario'
  AND json_extract(payload, '$.edad') > 30;

-- Agregación sobre campos JSON
SELECT json_extract(payload, '$.estado') as estado,
       COUNT(*) as total,
       SUM(json_extract(payload, '$.total')) as suma
FROM payload_default
WHERE entity = 'pedido'
GROUP BY 1
ORDER BY 2 DESC;
```

### JOINs

```sql
-- JOIN datos + metadatos de idempotencia
SELECT p.entity, p.key, p.version,
       i.idempotency_key, i.applied_at
FROM payload_default p
JOIN _idempotency i
  ON i.entity = p.entity AND i.key = p.key
WHERE p.entity = 'pedido'
ORDER BY i.applied_at DESC;

-- JOIN datos + eventos (outbox)
SELECT p.key,
       o.event_type,
       o.occurred_at,
       CASE WHEN o.published_at IS NULL THEN 'PENDING'
            ELSE 'PUBLISHED' END as status
FROM payload_default p
JOIN _outbox o
  ON o.entity = p.entity AND o.key = p.key
ORDER BY o.id DESC;

-- Cross-entity JOIN via foreign key en JSON
SELECT u.key as user_id,
       json_extract(u.payload, '$.nombre') as nombre,
       p.key as pedido_id,
       json_extract(p.payload, '$.total') as total,
       json_extract(p.payload, '$.estado') as estado
FROM payload_default u
JOIN payload_default p
  ON json_extract(p.payload, '$.usuario_id') = u.key
WHERE u.entity = 'usuario'
  AND p.entity = 'pedido'
ORDER BY u.key;

-- Pedidos por usuario (agregado)
SELECT json_extract(u.payload, '$.nombre') as usuario,
       COUNT(p.key) as num_pedidos,
       SUM(json_extract(p.payload, '$.total')) as total_gastado
FROM payload_default u
JOIN payload_default p
  ON json_extract(p.payload, '$.usuario_id') = u.key
WHERE u.entity = 'usuario' AND p.entity = 'pedido'
GROUP BY u.key
ORDER BY total_gastado DESC;
```

### Outbox y eventos

```sql
-- Eventos sin publicar
SELECT id, event_type, store, entity, key, occurred_at
FROM _outbox
WHERE published_at IS NULL
ORDER BY id;

-- Eventos publicados (últimos 20)
SELECT id, event_type, entity, key, published_at
FROM _outbox
WHERE published_at IS NOT NULL
ORDER BY id DESC
LIMIT 20;

-- Contar eventos por tipo
SELECT event_type, COUNT(*) as cnt
FROM _outbox
GROUP BY event_type
ORDER BY cnt DESC;

-- Limpieza de outbox antiguo (eventos con >7 días)
-- DELETE FROM _outbox WHERE published_at IS NOT NULL
--   AND published_at < datetime('now', '-7 days');
```

### Limpieza y mantenimiento

```sql
-- Eliminar registros de una entidad
DELETE FROM payload_default WHERE entity = 'test';

-- Eliminar todos los registros de prueba
DELETE FROM payload_default WHERE entity IN ('test', 'stress', 'large');

-- Verificar integridad
PRAGMA integrity_check;

-- Verificar WAL
PRAGMA journal_mode;  -- debe ser 'wal'

-- Tamaño de la BD
SELECT page_count * page_size as size_bytes
FROM pragma_page_count(), pragma_page_size();
```

### Reparación y debug

```sql
-- Ver registros de idempotencia
SELECT store, entity, key, version, applied_at
FROM _idempotency
ORDER BY applied_at DESC
LIMIT 20;

-- Buscar writes duplicados (misma entity+key, múltiples versiones)
SELECT entity, key, COUNT(*) as versions
FROM _idempotency
GROUP BY entity, key
HAVING COUNT(*) > 1;
```

## Operaciones vía API (alternativa a SQL directo)

```bash
# Health
curl http://192.168.3.175:30000/health | python3 -m json.tool

# Write
curl -X POST http://192.168.3.175:30000/write \
  -H "Authorization: Bearer smoke-test-key" \
  -H "Content-Type: application/json" \
  -d '{"op":"upsert","store":"default","entity":"test","key":"k1",
       "version":1,"ts":"2026-08-01T00:00:00Z",
       "idempotency_key":"'$(uuidgen)'","ack_mode":"accepted",
       "payload":{"data":"hello"}}'

# Status
curl http://192.168.3.175:30000/writes/default/<idempotency_key> \
  -H "Authorization: Bearer smoke-test-key"

# Read
curl "http://192.168.3.175:30000/read?store=default&entity=test&key=k1" \
  -H "Authorization: Bearer smoke-test-key"

# Métricas Prometheus
curl http://192.168.3.175:30000/metrics
```

-- seed-data.sql — Datos e-commerce de ejemplo para Ixmati
-- Carga via: podman exec ixmati-allinone sqlite3 /var/lib/ixmati/stores/default.db < seed-data.sql
-- O carga via API: for each row, POST /write con el payload correspondiente
--
-- NOTA: Este SQL es para referencia/documentacion. La carga real se hace via API.
-- Los INSERT directos omiten el outbox transaccional y la idempotencia.

-- ============================================
-- Schema (creado automaticamente por el writer)
-- ============================================

-- payload_default: datos de entidades
-- CREATE TABLE payload_default (
--     entity     TEXT NOT NULL,
--     key        TEXT NOT NULL,
--     version    INTEGER NOT NULL,
--     payload    BLOB NOT NULL,  -- JSON
--     updated_at TEXT NOT NULL DEFAULT (datetime('now')),
--     PRIMARY KEY (entity, key)
-- );

-- _idempotency: deduplicacion de comandos
-- CREATE TABLE _idempotency (
--     idempotency_key TEXT NOT NULL,
--     store           TEXT NOT NULL,
--     entity          TEXT NOT NULL,
--     key             TEXT NOT NULL,
--     version         INTEGER NOT NULL,
--     applied_at      TEXT NOT NULL DEFAULT (datetime('now')),
--     PRIMARY KEY (store, idempotency_key)
-- );

-- _outbox: eventos transaccionales
-- CREATE TABLE _outbox (
--     id            INTEGER PRIMARY KEY AUTOINCREMENT,
--     event_id      TEXT NOT NULL,
--     event_type    TEXT NOT NULL,
--     store         TEXT NOT NULL,
--     entity        TEXT NOT NULL,
--     key           TEXT NOT NULL,
--     version       INTEGER NOT NULL,
--     occurred_at   TEXT NOT NULL,
--     payload       BLOB NOT NULL,
--     published_at  TEXT
-- );

-- ============================================
-- Inserts de referencia (NO ejecutar directamente en produccion)
-- La carga correcta es via API para garantizar outbox + idempotencia
-- ============================================

-- Usuarios (5)
-- INSERT INTO payload_default (entity, key, version, payload) VALUES
-- ('usuario', 'u1', 1, '{"id":"u1","nombre":"Ana Garcia","email":"ana@example.com","edad":28}'),
-- ('usuario', 'u2', 1, '{"id":"u2","nombre":"Carlos Lopez","email":"carlos@example.com","edad":35}'),
-- ('usuario', 'u3', 1, '{"id":"u3","nombre":"Maria Ruiz","email":"maria@example.com","edad":22}'),
-- ('usuario', 'u4', 1, '{"id":"u4","nombre":"Pedro Diaz","email":"pedro@example.com","edad":41}'),
-- ('usuario', 'u5', 1, '{"id":"u5","nombre":"Laura Torres","email":"laura@example.com","edad":31}');

-- Productos (5)
-- INSERT INTO payload_default (entity, key, version, payload) VALUES
-- ('producto', 'p1', 1, '{"id":"p1","nombre":"Laptop Pro 15","precio":1299.99,"stock":45}'),
-- ('producto', 'p2', 1, '{"id":"p2","nombre":"Monitor 27 4K","precio":549.50,"stock":120}'),
-- ('producto', 'p3', 1, '{"id":"p3","nombre":"Teclado Mecanico","precio":89.99,"stock":300}'),
-- ('producto', 'p4', 1, '{"id":"p4","nombre":"Mouse Inalambrico","precio":45.00,"stock":500}'),
-- ('producto', 'p5', 1, '{"id":"p5","nombre":"Dock USB-C","precio":199.99,"stock":80}');

-- Pedidos (8)
-- INSERT INTO payload_default (entity, key, version, payload) VALUES
-- ('pedido', 'ped_1', 1, '{"id":"ped_1","usuario_id":"u1","total":1599.48,"estado":"entregado"}'),
-- ('pedido', 'ped_2', 1, '{"id":"ped_2","usuario_id":"u2","total":549.50,"estado":"entregado"}'),
-- ('pedido', 'ped_3', 1, '{"id":"ped_3","usuario_id":"u1","total":134.99,"estado":"enviado"}'),
-- ('pedido', 'ped_4', 1, '{"id":"ped_4","usuario_id":"u3","total":199.99,"estado":"confirmado"}'),
-- ('pedido', 'ped_5', 1, '{"id":"ped_5","usuario_id":"u5","total":1839.48,"estado":"pendiente"}'),
-- ('pedido', 'ped_6', 1, '{"id":"ped_6","usuario_id":"u2","total":45.00,"estado":"cancelado"}'),
-- ('pedido', 'ped_7', 1, '{"id":"ped_7","usuario_id":"u4","total":289.98,"estado":"procesando"}'),
-- ('pedido', 'ped_8', 1, '{"id":"ped_8","usuario_id":"u5","total":199.99,"estado":"confirmado"}');

-- ============================================
-- API endpoints para cargar datos correctamente
-- ============================================

-- Para cada entidad, enviar via API:
-- curl -X POST http://localhost:30000/write \
--   -H "Authorization: Bearer smoke-test-key" \
--   -H "Content-Type: application/json" \
--   -d '{
--     "op": "upsert",
--     "store": "default",
--     "entity": "usuario",
--     "key": "u1",
--     "version": 1,
--     "ts": "2026-08-01T00:00:00Z",
--     "idempotency_key": "<UUID>",
--     "ack_mode": "accepted",
--     "payload": {"id":"u1","nombre":"Ana Garcia","email":"ana@example.com","edad":28}
--   }'

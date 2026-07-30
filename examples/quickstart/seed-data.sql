-- seed-data.sql — inserts de ejemplo para explorar la BD manualmente
-- Uso: sqlite3 examples/quickstart/db/pedidos.db < examples/quickstart/seed-data.sql

INSERT OR REPLACE INTO _idempotency (idempotency_key, store, entity, key, version, applied_at)
VALUES
  ('seed-0001-0000-0000-000000000001', 'pedidos', 'pedido', 'ped_demo_1', 1, datetime('now')),
  ('seed-0002-0000-0000-000000000002', 'pedidos', 'pedido', 'ped_demo_2', 1, datetime('now'));

INSERT OR REPLACE INTO payload_pedidos (entity, key, version, payload, updated_at)
VALUES
  ('pedido', 'ped_demo_1', 1, '{"total": 1500.0, "estado": "pendiente", "cliente": "demo"}', datetime('now')),
  ('pedido', 'ped_demo_2', 1, '{"total": 2500.0, "estado": "confirmado", "cliente": "demo"}', datetime('now'));

INSERT OR REPLACE INTO _outbox (event_id, event_type, store, entity, key, version, occurred_at, payload)
VALUES
  ('evt-seed-001', 'pedido.creado', 'pedidos', 'pedido', 'ped_demo_1', 1, datetime('now'), '{"total": 1500}'),
  ('evt-seed-002', 'pedido.creado', 'pedidos', 'pedido', 'ped_demo_2', 1, datetime('now'), '{"total": 2500}');

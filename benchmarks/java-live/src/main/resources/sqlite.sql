PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA busy_timeout=5000;
CREATE TABLE IF NOT EXISTS payload_pedidos (entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,payload BLOB NOT NULL,updated_at TEXT NOT NULL DEFAULT (datetime('now')),PRIMARY KEY(entity,key));
CREATE TABLE IF NOT EXISTS _idempotency (idempotency_key TEXT PRIMARY KEY,store TEXT NOT NULL,entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,applied_at TEXT NOT NULL DEFAULT (datetime('now')));
CREATE TABLE IF NOT EXISTS _outbox (id INTEGER PRIMARY KEY AUTOINCREMENT,event_id TEXT NOT NULL UNIQUE,event_type TEXT NOT NULL,store TEXT NOT NULL,entity TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,payload BLOB NOT NULL,published_at TEXT);

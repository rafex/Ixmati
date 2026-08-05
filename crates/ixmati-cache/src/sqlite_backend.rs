use crate::CacheBackend;
use rusqlite::{Connection, params};
use std::sync::Mutex;

pub struct SqliteCacheBackend {
    conn: Mutex<Connection>,
}

impl SqliteCacheBackend {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let conn = Connection::open(db_path)
            .map_err(|e| format!("SqliteCache: open failed: {}", e))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=5000;
             CREATE TABLE IF NOT EXISTS _cache (
                 key TEXT PRIMARY KEY,
                 value BLOB NOT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE INDEX IF NOT EXISTS idx_cache_key ON _cache(key);"
        ).map_err(|e| format!("SqliteCache: schema failed: {}", e))?;

        tracing::info!(db = %db_path, "SqliteCacheBackend initialized");
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn internal_key(ns: &str, entity: &str, key: &str) -> String {
        if key.is_empty() {
            format!("{}:{}", ns, entity)
        } else {
            format!("{}:{}:{}", ns, entity, key)
        }
    }
}

impl CacheBackend for SqliteCacheBackend {
    fn get(&self, ns: &str, entity: &str, key: &str) -> Option<Vec<u8>> {
        let k = Self::internal_key(ns, entity, key);
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT value FROM _cache WHERE key = ?1",
            params![k],
            |row| row.get(0),
        ).ok()
    }

    fn set(&self, ns: &str, entity: &str, key: &str, value: &[u8]) {
        let k = Self::internal_key(ns, entity, key);
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO _cache (key, value, created_at) VALUES (?1, ?2, datetime('now'))",
                params![k, value],
            );
        }
    }

    fn del(&self, ns: &str, entity: &str, key: &str) {
        let k = Self::internal_key(ns, entity, key);
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute("DELETE FROM _cache WHERE key = ?1", params![k]);
        }
    }

    fn delete_by_prefix(&self, ns: &str, prefix: &str) {
        let like = format!("{}:{}%", ns, prefix);
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute("DELETE FROM _cache WHERE key LIKE ?1", params![like]);
        }
    }

    fn flush(&self, _store: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_cache_set_get_del() {
        let backend = SqliteCacheBackend::new(":memory:").unwrap();

        backend.set("cache", "test:datos:1", "", b"hello");
        let val = backend.get("cache", "test:datos:1", "");
        assert_eq!(val, Some(b"hello".to_vec()));

        backend.del("cache", "test:datos:1", "");
        assert_eq!(backend.get("cache", "test:datos:1", ""), None);
    }

    #[test]
    fn sqlite_cache_delete_by_prefix() {
        let backend = SqliteCacheBackend::new(":memory:").unwrap();

        backend.set("cache", "c:store1:a", "", b"a");
        backend.set("cache", "c:store1:b", "", b"b");
        backend.set("cache", "c:store2:c", "", b"c");

        backend.delete_by_prefix("cache", "c:store1");
        assert_eq!(backend.get("cache", "c:store1:a", ""), None);
        assert_eq!(backend.get("cache", "c:store1:b", ""), None);
        assert_eq!(backend.get("cache", "c:store2:c", ""), Some(b"c".to_vec()));
    }
}

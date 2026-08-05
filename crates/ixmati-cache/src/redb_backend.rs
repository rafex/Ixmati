use crate::CacheBackend;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Mutex;

const CACHE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("cache");

pub struct RedbCacheBackend {
    db: Mutex<Database>,
    readonly: bool,
}

impl RedbCacheBackend {
    pub fn new(path: &str) -> Result<Self, String> {
        let db = Database::create(path)
            .map_err(|e| format!("Redb: create failed: {}", e))?;

        let txn = db.begin_write().map_err(|e| format!("Redb: write txn: {}", e))?;
        txn.open_table(CACHE_TABLE)
            .map_err(|e| format!("Redb: open table: {}", e))?;
        txn.commit().map_err(|e| format!("Redb: commit: {}", e))?;

        tracing::info!(path = %path, "RedbCacheBackend initialized (read-write)");
        Ok(Self { db: Mutex::new(db), readonly: false })
    }

    pub fn new_readonly(path: &str) -> Result<Self, String> {
        let db = (0..15)
            .find_map(|attempt| {
                if attempt > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                Database::open(path).ok()
            })
            .ok_or_else(|| {
                format!("Redb: readonly open failed after retries: {}", path)
            })?;

        tracing::info!(path = %path, "RedbCacheBackend initialized (read-only)");
        Ok(Self { db: Mutex::new(db), readonly: true })
    }

    fn internal_key(ns: &str, entity: &str, key: &str) -> String {
        if key.is_empty() {
            format!("{}:{}", ns, entity)
        } else {
            format!("{}:{}:{}", ns, entity, key)
        }
    }
}

impl CacheBackend for RedbCacheBackend {
    fn get(&self, ns: &str, entity: &str, key: &str) -> Option<Vec<u8>> {
        let k = Self::internal_key(ns, entity, key);
        let guard = self.db.lock().ok()?;
        let txn = guard.begin_read().ok()?;
        let table = txn.open_table(CACHE_TABLE).ok()?;
        table.get(k.as_str()).ok().flatten().map(|v| v.value().to_vec())
    }

    fn set(&self, ns: &str, entity: &str, key: &str, value: &[u8]) {
        if self.readonly {
            return;
        }
        let k = Self::internal_key(ns, entity, key);
        let guard = match self.db.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let txn = match guard.begin_write() {
            Ok(t) => t,
            Err(_) => return,
        };
        {
            let mut table = match txn.open_table(CACHE_TABLE) {
                Ok(t) => t,
                Err(_) => return,
            };
            if table.insert(k.as_str(), value).is_err() {
                return;
            }
        }
        let _ = txn.commit();
    }

    fn del(&self, ns: &str, entity: &str, key: &str) {
        if self.readonly {
            return;
        }
        let k = Self::internal_key(ns, entity, key);
        let guard = match self.db.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let txn = match guard.begin_write() {
            Ok(t) => t,
            Err(_) => return,
        };
        {
            let mut table = match txn.open_table(CACHE_TABLE) {
                Ok(t) => t,
                Err(_) => return,
            };
            let _ = table.remove(k.as_str());
        }
        let _ = txn.commit();
    }

    fn delete_by_prefix(&self, ns: &str, prefix: &str) {
        if self.readonly {
            return;
        }
        let prefix = format!("{}:{}", ns, prefix);
        let guard = match self.db.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let keys: Vec<String> = {
            let read_txn = match guard.begin_read() {
                Ok(t) => t,
                Err(_) => return,
            };
            let table = match read_txn.open_table(CACHE_TABLE) {
                Ok(t) => t,
                Err(_) => return,
            };
            match table.iter() {
                Ok(iter) => iter
                    .flatten()
                    .filter_map(|(k, _)| {
                        let key_str = k.value();
                        if key_str.starts_with(&prefix) {
                            Some(key_str.to_string())
                        } else {
                            None
                        }
                    })
                    .collect(),
                Err(_) => return,
            }
        };

        let txn = match guard.begin_write() {
            Ok(t) => t,
            Err(_) => return,
        };
        {
            let mut table = match txn.open_table(CACHE_TABLE) {
                Ok(t) => t,
                Err(_) => return,
            };
            for k in keys {
                let _ = table.remove(k.as_str());
            }
        }
        let _ = txn.commit();
    }

    fn flush(&self, _store: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> String {
        let mut p = PathBuf::from(std::env::temp_dir());
        p.push(format!("ixmati-redb-test-{}", name));
        let _ = std::fs::remove_file(&p);
        p.to_str().unwrap().to_string()
    }

    #[test]
    fn redb_cache_set_get_del() {
        let path = tmp_path("sgd.redb");
        let backend = RedbCacheBackend::new(&path).unwrap();

        backend.set("cache", "test:datos:1", "", b"hello");
        let val = backend.get("cache", "test:datos:1", "");
        assert_eq!(val, Some(b"hello".to_vec()));

        backend.del("cache", "test:datos:1", "");
        assert_eq!(backend.get("cache", "test:datos:1", ""), None);
    }

    #[test]
    fn redb_readonly_reads() {
        let path = tmp_path("ro.redb");

        let rw = RedbCacheBackend::new(&path).unwrap();
        rw.set("cache", "t:e:k", "", b"data");
        drop(rw);

        let ro = RedbCacheBackend::new_readonly(&path).unwrap();
        assert_eq!(ro.get("cache", "t:e:k", ""), Some(b"data".to_vec()));
        assert_eq!(ro.get("cache", "t:e:zzz", ""), None);
    }
}

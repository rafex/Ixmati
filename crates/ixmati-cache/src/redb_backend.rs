use crate::CacheBackend;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

const CACHE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("cache");

/// Sin `Mutex` a propósito (ver DEC-0043/DEC-0044): `Database::begin_read`
/// toma `&self` y soporta lecturas concurrentes reales sin lock externo, y
/// `Database::begin_write` también toma `&self` — su propia documentación
/// dice literalmente "only a single write may be in progress at a time...
/// this function will block until it completes", es decir, redb ya
/// serializa las escrituras internamente vía su `transaction_tracker`. El
/// `Mutex<Database>` que había antes era redundante para escrituras y
/// activamente dañino para lecturas: forzaba a que todos los GET
/// (get/entre stores/tenants distintos) hicieran cola detrás de cualquier
/// operación en curso, en vez de correr en paralelo como redb permite.
pub struct RedbCacheBackend {
    db: Database,
    readonly: bool,
}

impl RedbCacheBackend {
    pub fn new(path: &str) -> Result<Self, String> {
        let db = Database::create(path).map_err(|e| format!("Redb: create failed: {}", e))?;

        let txn = db
            .begin_write()
            .map_err(|e| format!("Redb: write txn: {}", e))?;
        txn.open_table(CACHE_TABLE)
            .map_err(|e| format!("Redb: open table: {}", e))?;
        txn.commit().map_err(|e| format!("Redb: commit: {}", e))?;

        tracing::info!(path = %path, "RedbCacheBackend initialized (read-write)");
        Ok(Self {
            db,
            readonly: false,
        })
    }

    pub fn new_readonly(path: &str) -> Result<Self, String> {
        let db = (0..15)
            .find_map(|attempt| {
                if attempt > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                Database::open(path).ok()
            })
            .ok_or_else(|| format!("Redb: readonly open failed after retries: {}", path))?;

        tracing::info!(path = %path, "RedbCacheBackend initialized (read-only)");
        Ok(Self { db, readonly: true })
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
        let txn = self.db.begin_read().ok()?;
        let table = txn.open_table(CACHE_TABLE).ok()?;
        table
            .get(k.as_str())
            .ok()
            .flatten()
            .map(|v| v.value().to_vec())
    }

    fn set(&self, ns: &str, entity: &str, key: &str, value: &[u8]) {
        if self.readonly {
            return;
        }
        let k = Self::internal_key(ns, entity, key);
        let txn = match self.db.begin_write() {
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
        let txn = match self.db.begin_write() {
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
        let keys: Vec<String> = {
            let read_txn = match self.db.begin_read() {
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

        let txn = match self.db.begin_write() {
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
    fn tmp_path(name: &str) -> String {
        let mut p = std::env::temp_dir();
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

    #[test]
    fn internal_key_with_empty_value_field_produces_ns_colon_entity() {
        assert_eq!(
            RedbCacheBackend::internal_key("cache", "p:usuarios_materializados:usr_1", ""),
            "cache:p:usuarios_materializados:usr_1"
        );
    }

    #[test]
    fn internal_key_with_value_field_produces_ns_colon_entity_colon_key() {
        assert_eq!(
            RedbCacheBackend::internal_key("cache", "usuarios", "usr_1"),
            "cache:usuarios:usr_1"
        );
    }

    #[test]
    fn cache_server_set_get_roundtrip_matches_key_composition() {
        let path = tmp_path("roundtrip.redb");
        let backend = RedbCacheBackend::new(&path).unwrap();

        // Simula lo que hace el CacheServer: set("cache", raw_key, "", value)
        let raw_key = "p:usuarios_materializados:usr_1";
        backend.set("cache", raw_key, "", b"{\"nombre\":\"Ana\"}");

        // Simula lo que hace el CacheServer: get("cache", raw_key, "")
        let val = backend.get("cache", raw_key, "");
        assert_eq!(val, Some(b"{\"nombre\":\"Ana\"}".to_vec()));
    }

    #[test]
    fn projection_and_entity_keys_dont_collide() {
        let path = tmp_path("nocol.redb");
        let backend = RedbCacheBackend::new(&path).unwrap();

        // Proyección: clave compuesta por el server
        backend.set("cache", "p:usuarios_materializados:usr_1", "", b"proj_data");

        // Entidad cacheada por el writer: cache_key("usuarios", "usuario", "usr_1") = "c:usuarios:usuario:usr_1"
        backend.set("cache", "c:usuarios:usuario:usr_1", "", b"entity_data");

        // Deben ser claves independientes
        assert_eq!(
            backend.get("cache", "p:usuarios_materializados:usr_1", ""),
            Some(b"proj_data".to_vec())
        );
        assert_eq!(
            backend.get("cache", "c:usuarios:usuario:usr_1", ""),
            Some(b"entity_data".to_vec())
        );
    }
}

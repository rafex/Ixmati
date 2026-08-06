pub mod backend;
pub mod cache_client;
pub mod cache_server;
pub mod noop;
pub mod readonly;
pub mod redb_backend;
pub mod sqlite_backend;

#[cfg(feature = "flashdb")]
pub mod flashdb_store;

pub use backend::CacheBackend;
pub use cache_client::CacheClient;
pub use cache_server::CacheServer;
pub use noop::NoOpBackend;
pub use readonly::ReadOnlyCache;
pub use redb_backend::RedbCacheBackend;
pub use sqlite_backend::SqliteCacheBackend;

#[cfg(feature = "flashdb")]
pub use flashdb_store::FlashDb;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_get_returns_none() {
        let backend = NoOpBackend::new();
        assert!(backend.get("store", "entity", "key").is_none());
    }

    #[test]
    fn noop_set_does_not_crash() {
        let backend = NoOpBackend::new();
        backend.set("store", "entity", "key", &[1, 2, 3]);
    }

    #[test]
    fn noop_del_does_not_crash() {
        let backend = NoOpBackend::new();
        backend.del("store", "entity", "key");
    }

    #[test]
    fn noop_delete_by_prefix_does_not_crash() {
        let backend = NoOpBackend::new();
        backend.delete_by_prefix("store", "prefix:");
    }

    #[test]
    fn sqlite_cache_basic_ops() {
        let backend = SqliteCacheBackend::new(":memory:").unwrap();
        backend.set("cache", "t:e:k", "", b"val");
        assert_eq!(backend.get("cache", "t:e:k", ""), Some(b"val".to_vec()));
        backend.del("cache", "t:e:k", "");
        assert_eq!(backend.get("cache", "t:e:k", ""), None);
    }
}

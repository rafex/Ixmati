pub mod backend;
pub mod flashdb_store;
pub mod noop;

pub use backend::CacheBackend;
pub use flashdb_store::FlashDb;
pub use noop::NoOpBackend;

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
    #[cfg(not(feature = "flashdb"))]
    fn flashdb_stub_get_returns_none() {
        let backend = FlashDb::new("/tmp/test-stub").unwrap();
        assert!(backend.get("test", "entity", "key").is_none());
    }

    #[test]
    #[cfg(not(feature = "flashdb"))]
    fn flashdb_stub_set_del_does_not_crash() {
        let backend = FlashDb::new("/tmp/test-stub").unwrap();
        backend.set("test", "entity", "key", &[1, 2, 3]);
        backend.del("test", "entity", "key");
    }
}

pub mod backend;
pub mod noop;

pub use backend::CacheBackend;
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
}

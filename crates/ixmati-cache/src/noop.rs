use crate::backend::CacheBackend;

pub struct NoOpBackend;

impl NoOpBackend {
    pub fn new() -> Self {
        Self
    }
}

impl CacheBackend for NoOpBackend {
    fn get(&self, _store: &str, _entity: &str, _key: &str) -> Option<Vec<u8>> {
        None
    }

    fn set(&self, _store: &str, _entity: &str, _key: &str, _value: &[u8]) {}

    fn del(&self, _store: &str, _entity: &str, _key: &str) {}

    fn delete_by_prefix(&self, _store: &str, _prefix: &str) {}

    fn flush(&self, _store: &str) {}
}

impl Default for NoOpBackend {
    fn default() -> Self {
        Self::new()
    }
}

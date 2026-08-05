use crate::CacheBackend;

pub struct ReadOnlyCache<B: CacheBackend> {
    inner: B,
}

impl<B: CacheBackend> ReadOnlyCache<B> {
    pub fn new(inner: B) -> Self {
        Self { inner }
    }
}

impl<B: CacheBackend> CacheBackend for ReadOnlyCache<B> {
    fn get(&self, ns: &str, entity: &str, key: &str) -> Option<Vec<u8>> {
        self.inner.get(ns, entity, key)
    }

    fn set(&self, _ns: &str, _entity: &str, _key: &str, _value: &[u8]) {}

    fn del(&self, _ns: &str, _entity: &str, _key: &str) {}

    fn delete_by_prefix(&self, _ns: &str, _prefix: &str) {}

    fn flush(&self, _store: &str) {}
}

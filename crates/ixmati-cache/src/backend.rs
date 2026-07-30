pub trait CacheBackend: Send + Sync {
    fn get(&self, store: &str, entity: &str, key: &str) -> Option<Vec<u8>>;

    fn set(&self, store: &str, entity: &str, key: &str, value: &[u8]);

    fn del(&self, store: &str, entity: &str, key: &str);

    fn delete_by_prefix(&self, store: &str, prefix: &str);

    fn flush(&self, store: &str);
}

pub struct CacheEntry {
    pub store: String,
    pub entity: String,
    pub key: String,
    pub value: Vec<u8>,
}

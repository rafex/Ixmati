use ixmati_cache::CacheBackend;
use ixmati_core::WriteEnvelope;
use std::sync::Arc;

pub struct CacheSync {
    cache: Arc<dyn CacheBackend>,
}

impl CacheSync {
    pub fn new(cache: Arc<dyn CacheBackend>) -> Self {
        Self { cache }
    }

    pub fn invalidate_after_write(&self, cmd: &WriteEnvelope) {
        let key = cache_key(&cmd.store, &cmd.entity, &cmd.key);
        self.cache.del("cache", &key, "");
    }

    pub fn repopulate_after_write(&self, cmd: &WriteEnvelope) {
        let key = cache_key(&cmd.store, &cmd.entity, &cmd.key);
        let payload = serde_json::to_vec(&cmd.payload).unwrap_or_default();
        self.cache.set("cache", &key, "", &payload);
    }

    pub fn sync_batch(&self, commands: &[WriteEnvelope]) {
        tracing::info!(count = commands.len(), "CacheSync::sync_batch");
        for cmd in commands {
            match cmd.op.as_str() {
                "delete" => self.invalidate_after_write(cmd),
                _ => self.repopulate_after_write(cmd),
            }
        }
    }

    pub fn delete_by_store(&self, store: &str) {
        let prefix = format!("c:{}", store);
        self.cache.delete_by_prefix("cache", &prefix);
    }
}

pub fn cache_key(store: &str, entity: &str, key: &str) -> String {
    format!("c:{}:{}:{}", store, entity, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixmati_cache::NoOpBackend;
    use serde_json::json;

    #[test]
    fn cache_sync_does_not_crash_with_noop() {
        let cache = Arc::new(NoOpBackend::new());
        let sync = CacheSync::new(cache);

        let cmd = WriteEnvelope {
            op: "upsert".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_1".into(),
            version: 1,
            ts: "2026-07-30T00:00:00Z".into(),
            idempotency_key: "ik-1".into(),
            ack_mode: "committed".into(),
            payload: json!({"total": 100}),
        };

        sync.sync_batch(&[cmd]);
    }

    #[test]
    fn cache_sync_handles_delete() {
        let cache = Arc::new(NoOpBackend::new());
        let sync = CacheSync::new(cache);

        let cmd = WriteEnvelope {
            op: "delete".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_1".into(),
            version: 2,
            ts: "2026-07-30T00:00:00Z".into(),
            idempotency_key: "ik-2".into(),
            ack_mode: "committed".into(),
            payload: json!({}),
        };

        sync.invalidate_after_write(&cmd);
    }

    #[test]
    fn cache_key_generation() {
        let key = cache_key("pedidos", "pedido", "ped_abc");
        assert_eq!(key, "c:pedidos:pedido:ped_abc");
    }

    #[test]
    fn store_prefix_generation() {
        let sync = CacheSync::new(Arc::new(NoOpBackend::new()));
        sync.delete_by_store("pedidos");
    }
}

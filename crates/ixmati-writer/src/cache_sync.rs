use ixmati_cache::CacheClient;
use ixmati_core::WriteEnvelope;
use std::sync::Arc;

pub struct CacheSync {
    client: Arc<CacheClient>,
}

impl CacheSync {
    pub fn new(client: Arc<CacheClient>) -> Self {
        Self { client }
    }

    pub fn invalidate_after_write(&self, store: &str, entity: &str, key: &str) {
        let cache_key = CacheClient::cache_key(store, entity, key);
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.client.del_raw(&cache_key).await;
            });
        });
    }

    pub fn repopulate_after_write(&self, store: &str, entity: &str, key: &str, payload: &serde_json::Value) {
        let cache_key = CacheClient::cache_key(store, entity, key);
        let value = serde_json::to_vec(payload).unwrap_or_default();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.client.set_raw(&cache_key, &value).await;
            });
        });
    }

    pub fn sync_batch(&self, commands: &[WriteEnvelope]) {
        tracing::info!(count = commands.len(), "CacheSync::sync_batch");
        for cmd in commands {
            match cmd.op.as_str() {
                "delete" => self.invalidate_after_write(&cmd.store, &cmd.entity, &cmd.key),
                _ => self.repopulate_after_write(
                    &cmd.store,
                    &cmd.entity,
                    &cmd.key,
                    &cmd.payload,
                ),
            }
        }
    }

    pub fn delete_by_store(&self, store: &str) {
        let prefix = format!("c:{store}");
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.client.delete_by_prefix(&prefix).await;
            });
        });
    }
}

pub fn cache_key(store: &str, entity: &str, key: &str) -> String {
    CacheClient::cache_key(store, entity, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cache_key_generation() {
        let key = cache_key("pedidos", "pedido", "ped_abc");
        assert_eq!(key, "c:pedidos:pedido:ped_abc");
    }
}

use ixmati_cache::SyncCacheClient;
use ixmati_core::WriteEnvelope;
use std::sync::Arc;

/// Sincroniza el cache-server tras cada batch escrito (ver DEC-0052). Sin
/// tokio: llamadas bloqueantes secuenciales sobre `SyncCacheClient`. El
/// cliente async anterior despachaba concurrentemente vía `JoinSet`, pero
/// `CacheClient`/`SyncCacheClient` serializan igual todas las escrituras
/// sobre un único socket con un mutex — esa concurrencia nunca lograba
/// paralelismo real de I/O (ver comentario histórico en DEC-0050), así que
/// secuencial acá no es una regresión de rendimiento.
pub struct CacheSync {
    client: Arc<SyncCacheClient>,
}

impl CacheSync {
    pub fn new(client: Arc<SyncCacheClient>) -> Self {
        Self { client }
    }

    pub fn invalidate_after_write(&self, store: &str, entity: &str, key: &str) {
        let cache_key = SyncCacheClient::cache_key(store, entity, key);
        self.client.del_raw(&cache_key);
    }

    pub fn repopulate_after_write(
        &self,
        store: &str,
        entity: &str,
        key: &str,
        payload: &serde_json::Value,
    ) {
        let cache_key = SyncCacheClient::cache_key(store, entity, key);
        let value = serde_json::to_vec(payload).unwrap_or_default();
        self.client.set_raw(&cache_key, &value);
    }

    pub fn sync_batch(&self, commands: &[WriteEnvelope]) -> usize {
        tracing::info!(count = commands.len(), "CacheSync::sync_batch");
        let mut errors = 0;
        for cmd in commands {
            let cache_key = SyncCacheClient::cache_key(&cmd.store, &cmd.entity, &cmd.key);
            match cmd.op.as_str() {
                "delete" => {
                    if !self.client.del_raw_checked(&cache_key) {
                        errors += 1;
                    }
                }
                _ => {
                    let value = serde_json::to_vec(&cmd.payload).unwrap_or_default();
                    if !self.client.set_raw_checked(&cache_key, &value) {
                        errors += 1;
                    }
                }
            }
        }
        errors
    }

    pub fn delete_by_store(&self, store: &str) {
        let prefix = format!("c:{store}");
        self.client.delete_by_prefix(&prefix);
    }
}

pub fn cache_key(store: &str, entity: &str, key: &str) -> String {
    SyncCacheClient::cache_key(store, entity, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_generation() {
        let key = cache_key("pedidos", "pedido", "ped_abc");
        assert_eq!(key, "c:pedidos:pedido:ped_abc");
    }
}

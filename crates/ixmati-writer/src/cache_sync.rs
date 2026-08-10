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

    pub async fn invalidate_after_write(&self, store: &str, entity: &str, key: &str) {
        let cache_key = CacheClient::cache_key(store, entity, key);
        self.client.del_raw(&cache_key).await;
    }

    pub async fn repopulate_after_write(
        &self,
        store: &str,
        entity: &str,
        key: &str,
        payload: &serde_json::Value,
    ) {
        let cache_key = CacheClient::cache_key(store, entity, key);
        let value = serde_json::to_vec(payload).unwrap_or_default();
        self.client.set_raw(&cache_key, &value).await;
    }

    /// Antes: loop secuencial, una llamada bloqueante (`block_in_place` +
    /// `block_on`, innecesario — este método ya se llama desde contexto
    /// async) por cada comando del batch. Con hasta `batch_size` comandos
    /// (default 100), eso eran ~100 round-trips al socket del cache-server
    /// uno detrás del otro — el `block_in_place`/`block_on` en sí agregaba
    /// overhead de salto a thread-pool bloqueante por cada uno. Medido con
    /// `WRITE_BATCH_DURATION` (ver DEC-0050): la transacción SQLite tomaba
    /// ~312ms por batch pero el ciclo completo ~1.18s — la diferencia era
    /// este método, no SQLite. Ahora se despacha todo el batch
    /// concurrentemente; `CacheClient` serializa igual sus escrituras sobre
    /// un único socket (`Arc<Mutex<UnixStream>>`), así que esto no logra
    /// paralelismo real de I/O, pero elimina el overhead de
    /// `block_in_place` por comando y el tiempo muerto entre awaits
    /// secuenciales del código propio.
    pub async fn sync_batch(&self, commands: &[WriteEnvelope]) {
        tracing::info!(count = commands.len(), "CacheSync::sync_batch");
        let mut tasks = tokio::task::JoinSet::new();
        for cmd in commands {
            let client = Arc::clone(&self.client);
            let cmd = cmd.clone();
            tasks.spawn(async move {
                let cache_key = CacheClient::cache_key(&cmd.store, &cmd.entity, &cmd.key);
                match cmd.op.as_str() {
                    "delete" => client.del_raw(&cache_key).await,
                    _ => {
                        let value = serde_json::to_vec(&cmd.payload).unwrap_or_default();
                        client.set_raw(&cache_key, &value).await;
                    }
                }
            });
        }
        while tasks.join_next().await.is_some() {}
    }

    pub async fn delete_by_store(&self, store: &str) {
        let prefix = format!("c:{store}");
        self.client.delete_by_prefix(&prefix).await;
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

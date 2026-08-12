use ixmati_cache::SyncCacheClient;
use ixmati_core::{EventEnvelope, WriteEnvelope};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::time::Instant;

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

struct CacheSyncJob {
    commands: Vec<WriteEnvelope>,
}

/// Post-commit cache projection worker.
///
/// SQLite `_idempotency` and `_outbox` are the durable boundary.  Cache is a
/// rebuildable projection and therefore must not block the writer loop after a
/// successful transaction.  The queue is bounded deliberately: if it fills,
/// the event remains recoverable from `_outbox` and the missed cache update is
/// made visible for reconciliation instead of growing memory without limit.
#[derive(Clone)]
pub struct CacheSyncWorker {
    tx: SyncSender<CacheSyncJob>,
    depth: Arc<AtomicU64>,
    store: Arc<str>,
    capacity: usize,
}

impl CacheSyncWorker {
    pub fn spawn(client: Arc<SyncCacheClient>, capacity: usize, store: &str) -> Self {
        let capacity = capacity.max(1);
        let (tx, rx) = mpsc::sync_channel::<CacheSyncJob>(capacity);
        let depth = Arc::new(AtomicU64::new(0));
        let worker_depth = Arc::clone(&depth);
        let worker_store: Arc<str> = Arc::from(store.to_owned());
        let worker_store_for_thread = Arc::clone(&worker_store);

        std::thread::Builder::new()
            .name("ixmati-cache-sync".into())
            .spawn(move || {
                let sync = CacheSync::new(client);
                while let Ok(job) = rx.recv() {
                    worker_depth.fetch_sub(1, Ordering::Relaxed);
                    record_depth(&worker_depth, &worker_store_for_thread);

                    let started = Instant::now();
                    let errors = sync.sync_batch(&job.commands);
                    if errors > 0 {
                        crate::metrics::CACHE_SYNC_ERRORS.add(
                            errors as u64,
                            &[opentelemetry::KeyValue::new(
                                "store",
                                worker_store_for_thread.to_string(),
                            )],
                        );
                    }
                    crate::metrics::CACHE_SYNC_DURATION.record(
                        started.elapsed().as_secs_f64(),
                        &[opentelemetry::KeyValue::new(
                            "store",
                            worker_store_for_thread.to_string(),
                        )],
                    );
                }

                tracing::warn!(
                    store = %worker_store_for_thread,
                    "cache sync worker stopped"
                );
            })
            .expect("failed to spawn cache sync worker");

        record_depth(&depth, &worker_store);
        Self {
            tx,
            depth,
            store: worker_store,
            capacity,
        }
    }

    /// Enqueue a post-commit projection without delaying the SQLite writer.
    /// Returns false only when the bounded projection queue is full or closed.
    pub fn try_enqueue(&self, commands: Vec<WriteEnvelope>) -> bool {
        if commands.is_empty() {
            return true;
        }

        self.depth.fetch_add(1, Ordering::Relaxed);
        record_depth(&self.depth, &self.store);
        match self.tx.try_send(CacheSyncJob { commands }) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.depth.fetch_sub(1, Ordering::Relaxed);
                record_depth(&self.depth, &self.store);
                crate::metrics::CACHE_SYNC_DEFERRED.add(
                    1,
                    &[opentelemetry::KeyValue::new(
                        "store",
                        self.store.to_string(),
                    )],
                );
                tracing::warn!(
                    store = %self.store,
                    capacity = self.capacity,
                    "cache sync queue full; event remains recoverable from outbox"
                );
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.depth.fetch_sub(1, Ordering::Relaxed);
                record_depth(&self.depth, &self.store);
                crate::metrics::CACHE_SYNC_DEFERRED.add(
                    1,
                    &[opentelemetry::KeyValue::new(
                        "store",
                        self.store.to_string(),
                    )],
                );
                tracing::error!(store = %self.store, "cache sync worker disconnected");
                false
            }
        }
    }

    pub fn depth(&self) -> u64 {
        self.depth.load(Ordering::Relaxed)
    }
}

fn record_depth(depth: &AtomicU64, store: &str) {
    crate::metrics::CACHE_SYNC_QUEUE_DEPTH.record(
        depth.load(Ordering::Relaxed),
        &[opentelemetry::KeyValue::new("store", store.to_owned())],
    );
}

pub fn event_to_cache_command(event: &EventEnvelope) -> WriteEnvelope {
    let op = if event.event_type.ends_with(".eliminado") {
        "delete"
    } else {
        "upsert"
    };
    WriteEnvelope {
        op: op.into(),
        store: event.store.clone(),
        entity: event.entity.clone(),
        key: event.key.clone(),
        version: event.version,
        ts: event.occurred_at.clone(),
        idempotency_key: String::new(),
        ack_mode: "accepted".into(),
        payload: event.payload.clone(),
    }
}

pub fn cache_key(store: &str, entity: &str, key: &str) -> String {
    SyncCacheClient::cache_key(store, entity, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn cache_key_generation() {
        let key = cache_key("pedidos", "pedido", "ped_abc");
        assert_eq!(key, "c:pedidos:pedido:ped_abc");
    }

    #[test]
    fn delete_event_is_not_repopulated_into_cache() {
        let event = EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "pedido.eliminado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped-1".into(),
            version: 2,
            occurred_at: Utc::now().to_rfc3339(),
            outbox_seq: 1,
            payload: serde_json::json!({"id": "ped-1"}),
        };

        assert_eq!(event_to_cache_command(&event).op, "delete");
    }

    #[test]
    fn update_event_is_repopulated_into_cache() {
        let event = EventEnvelope {
            event_id: "evt-2".into(),
            event_type: "pedido.actualizado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped-1".into(),
            version: 3,
            occurred_at: Utc::now().to_rfc3339(),
            outbox_seq: 2,
            payload: serde_json::json!({"id": "ped-1", "total": 42}),
        };

        assert_eq!(event_to_cache_command(&event).op, "upsert");
    }
}

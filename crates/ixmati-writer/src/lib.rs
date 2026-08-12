pub mod batcher;
pub mod cache_responder;
pub mod cache_server;
pub mod cache_sync;
pub mod cdc;
pub mod consumer;
pub mod db;
pub mod dedup;
pub mod event_publisher;
pub mod metrics;
pub mod outbox;
pub mod standby;
pub mod tombstones;
pub mod watchdog;
pub mod write_engine;
pub mod write_thread;

use ixmati_core::{EventEnvelope, StoreConfig};

use crate::consumer::PendingCommand;

pub struct Writer {
    pub store_name: String,
    pub db_path: String,
}

impl Writer {
    pub fn new(config: &StoreConfig) -> Self {
        Self {
            store_name: config.name.clone(),
            db_path: config.db_path.clone(),
        }
    }

    pub fn store_name(&self) -> &str {
        &self.store_name
    }

    pub fn db_path(&self) -> &str {
        &self.db_path
    }
}

pub struct Batch {
    pub commands: Vec<PendingCommand>,
}

impl Batch {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn push(&mut self, cmd: PendingCommand) {
        self.commands.push(cmd);
    }

    pub fn drain(&mut self) -> Vec<PendingCommand> {
        std::mem::take(&mut self.commands)
    }
}

impl Default for Batch {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BatchResult {
    pub store: String,
    pub committed: usize,
    pub duplicates: usize,
    pub version_conflicts: usize,
    pub events: Vec<EventEnvelope>,
    /// Filled by `WriteHandle`; direct `WriteEngine` calls leave these at 0.
    pub write_queue_wait_seconds: f64,
    pub sqlite_process_seconds: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_creation() {
        let config = ixmati_core::StoreConfig {
            name: "pedidos".into(),
            label: None,
            db_path: "/data/pedidos.db".into(),
            topic_cmd: String::new(),
            topic_evt: String::new(),
            mqtt_broker: "tcp://localhost:1883".into(),
            mqtt_client_id: "writer-pedidos".into(),
            batch_size: 100,
            batch_interval_ms: 50,
            litestream_config: None,
        };

        let writer = Writer::new(&config);
        assert_eq!(writer.store_name(), "pedidos");
        assert_eq!(writer.db_path(), "/data/pedidos.db");
    }

    #[test]
    fn batch_collects_commands() {
        let mut batch = Batch::new();
        assert!(batch.is_empty());

        let (client, _connection) = rumqttc::Client::new(
            rumqttc::MqttOptions::new("ixmati-test", "localhost", 1883),
            10,
        );
        batch.push(PendingCommand {
            envelope: ixmati_core::WriteEnvelope {
                op: "upsert".into(),
                store: "pedidos".into(),
                entity: "pedido".into(),
                key: "p1".into(),
                version: 1,
                ts: "2026-07-30T00:00:00Z".into(),
                idempotency_key: "a".into(),
                ack_mode: "committed".into(),
                payload: serde_json::json!({"total": 100}),
            },
            ack: crate::consumer::MqttAck {
                client,
                publish: rumqttc::Publish::new(
                    "ixmati/test",
                    rumqttc::QoS::AtLeastOnce,
                    Vec::new(),
                ),
            },
        });

        assert_eq!(batch.len(), 1);
        let drained = batch.drain();
        assert_eq!(drained.len(), 1);
        assert!(batch.is_empty());
    }
}

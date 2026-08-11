use std::time::{Duration, Instant};

use crate::Batch;
use crate::consumer::PendingCommand;

pub struct Batcher {
    batch_size: usize,
    batch_interval: Duration,
    current: Batch,
    last_flush: Instant,
}

impl Batcher {
    pub fn new(batch_size: usize, batch_interval_ms: u64) -> Self {
        Self {
            batch_size,
            batch_interval: Duration::from_millis(batch_interval_ms),
            current: Batch::new(),
            last_flush: Instant::now(),
        }
    }

    pub fn push(&mut self, cmd: PendingCommand) -> Option<Batch> {
        self.current.push(cmd);

        if self.current.len() >= self.batch_size {
            return Some(self.flush());
        }

        None
    }

    pub fn should_flush(&self) -> bool {
        !self.current.is_empty() && self.last_flush.elapsed() >= self.batch_interval
    }

    pub fn flush(&mut self) -> Batch {
        let mut batch = Batch::new();
        batch.commands = self.current.drain();
        self.last_flush = Instant::now();
        batch
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixmati_core::WriteEnvelope;
    use std::time::Duration;

    fn make_cmd(id: &str) -> WriteEnvelope {
        WriteEnvelope {
            op: "upsert".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: id.into(),
            version: 1,
            ts: "2026-07-30T00:00:00Z".into(),
            idempotency_key: format!("idem-{}", id),
            ack_mode: "committed".into(),
            payload: serde_json::json!({"n": 1}),
        }
    }

    #[test]
    fn flushes_when_batch_full() {
        let mut batcher = Batcher::new(2, 5000);
        assert!(batcher.push(make_pending("a")).is_none());
        assert!(batcher.push(make_pending("b")).is_some());
        assert!(batcher.is_empty());
    }

    #[test]
    fn respects_batch_size_boundary() {
        let mut batcher = Batcher::new(3, 5000);
        assert!(batcher.push(make_pending("a")).is_none());
        assert!(batcher.push(make_pending("b")).is_none());
        let flushed = batcher.push(make_pending("c")).unwrap();
        assert_eq!(flushed.len(), 3);
    }

    #[test]
    fn should_flush_after_interval() {
        let mut batcher = Batcher::new(100, 10);
        batcher.push(make_pending("a"));
        assert!(!batcher.should_flush());

        std::thread::sleep(Duration::from_millis(15));
        assert!(batcher.should_flush());

        let batch = batcher.flush();
        assert_eq!(batch.len(), 1);
        assert!(!batcher.should_flush());
    }

    fn make_pending(id: &str) -> PendingCommand {
        let (client, _connection) = rumqttc::Client::new(
            rumqttc::MqttOptions::new("ixmati-batcher-test", "localhost", 1883),
            10,
        );
        PendingCommand {
            envelope: make_cmd(id),
            ack: crate::consumer::MqttAck {
                client,
                publish: rumqttc::Publish::new(
                    "ixmati/test",
                    rumqttc::QoS::AtLeastOnce,
                    Vec::new(),
                ),
            },
        }
    }
}

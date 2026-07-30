use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::sync::Arc;

use crate::outbox::Outbox;

pub struct EventPublisher {
    client: AsyncClient,
    db_path: String,
}

impl EventPublisher {
    pub fn new(broker: &str, client_id: &str, db_path: &str) -> Self {
        let mut mqtt_options = MqttOptions::new(client_id, broker, 1883);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

        let (client, _eventloop) = AsyncClient::new(mqtt_options, 100);

        Self {
            client,
            db_path: db_path.to_string(),
        }
    }

    pub async fn publish_unpublished(&self, store: &str, limit: usize) -> rusqlite::Result<usize> {
        let conn = rusqlite::Connection::open(&self.db_path)?;

        let rows = Outbox::fetch_unpublished(&conn, store, limit)?;
        let mut published = 0;

        for row in &rows {
            let topic = format!(
                "ixmati/evt/{}/{}/{}",
                row.store, row.entity, row.key
            );

            let payload = row.payload.clone();

            match self
                .client
                .publish(&topic, QoS::AtLeastOnce, false, payload)
                .await
            {
                Ok(_) => {
                    let conn2 = rusqlite::Connection::open(&self.db_path)?;
                    Outbox::mark_published(&conn2, row.id)?;
                    published += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        event_id = %row.event_id,
                        store = %row.store,
                        "failed to publish event"
                    );
                }
            }
        }

        Ok(published)
    }

    pub async fn publish_loop(
        publisher: Arc<EventPublisher>,
        store: String,
        interval_ms: u64,
    ) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(interval_ms));

        loop {
            interval.tick().await;

            match publisher.publish_unpublished(&store, 100).await {
                Ok(count) if count > 0 => {
                    tracing::info!(store = %store, count, "published events");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(error = %e, store = %store, "event publisher error");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::outbox::Outbox;
    use ixmati_core::EventEnvelope;
    use rusqlite::Connection;

    fn make_event() -> EventEnvelope {
        EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "pedido.creado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_1".into(),
            version: 1,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: serde_json::json!({"total": 100}),
        }
    }

    #[test]
    fn publisher_can_mark_published() {
        let conn = Connection::open_in_memory().unwrap();
        Outbox::ensure_schema(&conn).unwrap();
        Outbox::insert(&conn, &make_event()).unwrap();

        let rows = Outbox::fetch_unpublished(&conn, "pedidos", 10).unwrap();
        assert_eq!(rows.len(), 1);

        Outbox::mark_published(&conn, rows[0].id).unwrap();

        let rows = Outbox::fetch_unpublished(&conn, "pedidos", 10).unwrap();
        assert!(rows.is_empty());
    }
}

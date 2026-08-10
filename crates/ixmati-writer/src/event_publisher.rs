use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::sync::Arc;

use crate::outbox::Outbox;

pub struct EventPublisher {
    client: AsyncClient,
    db_path: String,
}

impl EventPublisher {
    pub fn new(broker: &str, client_id: &str, db_path: &str) -> Self {
        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);
        let mut mqtt_options = MqttOptions::new(
            format!("{}-publisher", client_id),
            &host,
            port,
        );
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

        let (client, mut eventloop) = AsyncClient::new(mqtt_options, 100);

        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(_) => continue,
                    Err(e) => {
                        tracing::error!(error = %e, "MQTT publisher eventloop error");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        Self {
            client,
            db_path: db_path.to_string(),
        }
    }

    /// Publica hasta `limit` eventos pendientes del outbox a MQTT en paralelo
    /// y marca los exitosos con un único UPDATE en batch.
    ///
    /// Antes: una conexión SQLite nueva + un UPDATE individual por cada fila
    /// publicada, y publicación secuencial con `.await` por evento — con QoS 1
    /// (espera de PUBACK) eso topeaba el drenado muy por debajo del throughput
    /// de entrada bajo carga real (ver DEC-0043: ~100 eventos/s de techo con
    /// 448 writes/s de entrada, backlog creciendo sin límite). Ahora se
    /// publican todos los eventos del batch concurrentemente y se hace un
    /// solo UPDATE ... WHERE id IN (...) al final.
    pub async fn publish_unpublished(&self, store: &str, limit: usize) -> rusqlite::Result<usize> {
        let rows = {
            let conn = rusqlite::Connection::open(&self.db_path)?;
            Outbox::fetch_unpublished(&conn, store, limit)?
        };

        if rows.is_empty() {
            return Ok(0);
        }

        let mut publishes = tokio::task::JoinSet::new();
        for row in rows {
            let client = self.client.clone();
            publishes.spawn(async move {
                let topic = format!("ixmati/evt/{}/{}/{}", row.store, row.entity, row.key);
                let result = client
                    .publish(&topic, QoS::AtLeastOnce, false, row.payload.clone())
                    .await;
                (row.id, row.event_id, row.store, result)
            });
        }

        let mut published_ids = Vec::new();
        while let Some(joined) = publishes.join_next().await {
            let (id, event_id, event_store, result) = match joined {
                Ok(outcome) => outcome,
                Err(e) => {
                    tracing::error!(error = %e, "publish task panicked");
                    continue;
                }
            };
            match result {
                Ok(_) => published_ids.push(id),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        event_id = %event_id,
                        store = %event_store,
                        "failed to publish event"
                    );
                }
            }
        }

        let published = published_ids.len();
        if !published_ids.is_empty() {
            let conn = rusqlite::Connection::open(&self.db_path)?;
            Outbox::mark_published_batch(&conn, &published_ids)?;
        }

        Ok(published)
    }

    pub async fn publish_loop(
        publisher: Arc<EventPublisher>,
        store: String,
        interval_ms: u64,
        batch_limit: usize,
    ) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(interval_ms));

        loop {
            interval.tick().await;

            match publisher.publish_unpublished(&store, batch_limit).await {
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

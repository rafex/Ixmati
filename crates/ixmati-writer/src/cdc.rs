use ixmati_core::EventEnvelope;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::time::Duration;

pub struct CdcPublisher {
    client: AsyncClient,
}

impl CdcPublisher {
    pub async fn new(broker: &str, client_id: &str) -> Result<Self, String> {
        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);

        let mut opts = MqttOptions::new(client_id, &host, port);
        opts.set_keep_alive(Duration::from_secs(5));

        let (client, mut eventloop) = AsyncClient::new(opts, 100);

        tokio::spawn(async move {
            loop {
                if eventloop.poll().await.is_err() {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        });

        Ok(Self { client })
    }

    pub async fn publish(&self, _store: &str, events: &[EventEnvelope]) -> Result<(), String> {
        for event in events {
            let topic = format!("ixmati/evt/{}/{}/{}", event.store, event.entity, event.key);

            let payload =
                serde_json::to_vec(event).map_err(|e| format!("CDC serialization error: {}", e))?;

            self.client
                .publish(&topic, QoS::AtLeastOnce, false, payload)
                .await
                .map_err(|e| format!("CDC publish error: {}", e))?;
        }
        Ok(())
    }
}

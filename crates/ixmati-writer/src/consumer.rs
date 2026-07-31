use ixmati_core::WriteEnvelope;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::mpsc;

pub struct MqttConsumer {
    rx: mpsc::UnboundedReceiver<WriteEnvelope>,
}

impl MqttConsumer {
    pub fn new(
        broker: &str,
        client_id: &str,
        topic: &str,
    ) -> (Self, mpsc::UnboundedSender<WriteEnvelope>) {
        let (tx, rx) = mpsc::unbounded_channel();

        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);
        let mut mqtt_options = MqttOptions::new(client_id, &host, port);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

        let (client, mut eventloop) = AsyncClient::new(mqtt_options, 100);

        let topic_owned = topic.to_string();
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            client.subscribe(&topic_owned, QoS::AtLeastOnce).await.ok();

            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        let payload = publish.payload.to_vec();
                        match serde_json::from_slice::<WriteEnvelope>(&payload) {
                            Ok(envelope) => {
                                let _ = tx_clone.send(envelope);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    topic = %publish.topic,
                                    "failed to deserialize command"
                                );
                            }
                        }
                    }
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        tracing::info!(
                            topic = %topic_owned,
                            "MQTT consumer connected and subscribed"
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!(error = %e, "MQTT event loop error");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        });

        let consumer = Self { rx };
        (consumer, tx)
    }

    pub fn receiver(&mut self) -> &mut mpsc::UnboundedReceiver<WriteEnvelope> {
        &mut self.rx
    }
}

use ixmati_core::WriteEnvelope;
use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
use std::sync::mpsc;

/// Antes: `mpsc::unbounded_channel()` de tokio entre el eventloop de MQTT y
/// el loop principal de batching/SQLite (ver DEC-0048/TASK-VAL-0016). Si el
/// loop principal no podía procesar al ritmo de llegada, el canal crecía
/// sin límite — cada `WriteEnvelope` retiene su payload deserializado en
/// memoria, y esto fue el mecanismo concreto detrás de los OOM-kill del
/// writer bajo carga sostenida real.
///
/// Ahora (DEC-0052): sin tokio en el proceso. El canal es
/// `std::sync::mpsc::sync_channel` (acotado, bloqueante) y el cliente MQTT
/// usa `manual_acks`: solo se confirma al broker (`client.ack`) lo que
/// efectivamente entró en el canal. Si el canal está lleno, el mensaje
/// queda sin confirmar — se redistribuye según la semántica QoS1 normal, y
/// la backpressure recae en el propio broker (que ya tiene su límite
/// conocido, `max_queued_messages` en `mosquitto.conf`) en vez de
/// acumularse sin límite en RAM del writer.
const DEFAULT_CHANNEL_CAPACITY: usize = 5000;

pub struct MqttConsumer {
    rx: mpsc::Receiver<WriteEnvelope>,
}

impl MqttConsumer {
    pub fn new(broker: &str, client_id: &str, topic: &str) -> (Self, mpsc::SyncSender<WriteEnvelope>) {
        Self::with_capacity(broker, client_id, topic, DEFAULT_CHANNEL_CAPACITY)
    }

    /// Arranca un hilo de SO dedicado que conduce la conexión MQTT
    /// (`rumqttc::Connection`, API síncrona — internamente usa su propio
    /// runtime de tokio de un solo hilo, confinado por completo a este
    /// hilo, sin compartirse con el resto del proceso). Ningún canal de
    /// tokio cruza hacia el resto del sistema — solo `std::sync::mpsc`.
    pub fn with_capacity(
        broker: &str,
        client_id: &str,
        topic: &str,
        capacity: usize,
    ) -> (Self, mpsc::SyncSender<WriteEnvelope>) {
        let (tx, rx) = mpsc::sync_channel(capacity);

        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);
        let mut mqtt_options = MqttOptions::new(client_id, &host, port);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));
        mqtt_options.set_manual_acks(true);

        let (client, mut connection) = Client::new(mqtt_options, 100);

        let topic_owned = topic.to_string();
        let tx_clone = tx.clone();

        std::thread::Builder::new()
            .name("ixmati-mqtt-consumer".into())
            .spawn(move || {
                if let Err(e) = client.subscribe(&topic_owned, QoS::AtLeastOnce) {
                    tracing::error!(error = %e, "failed to subscribe to command topic");
                }

                for notification in connection.iter() {
                    match notification {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            match serde_json::from_slice::<WriteEnvelope>(&publish.payload) {
                                Ok(envelope) => match tx_clone.try_send(envelope) {
                                    Ok(()) => {
                                        if let Err(e) = client.ack(&publish) {
                                            tracing::warn!(error = %e, "failed to ack MQTT publish");
                                        }
                                    }
                                    Err(mpsc::TrySendError::Full(_)) => {
                                        // No se acked a propósito: el mensaje
                                        // queda pendiente en el broker (QoS1)
                                        // y se redistribuye. La backpressure
                                        // real vive acá, no como memoria sin
                                        // límite.
                                        tracing::warn!(
                                            capacity,
                                            "consumer channel full, deferring ack (backpressure)"
                                        );
                                    }
                                    Err(mpsc::TrySendError::Disconnected(_)) => {
                                        tracing::error!(
                                            "consumer channel closed, dropping message"
                                        );
                                    }
                                },
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        topic = %publish.topic,
                                        "failed to deserialize command"
                                    );
                                    // Mensaje envenenado: reintentar no lo va
                                    // a arreglar. Ackear para no
                                    // redistribuirlo por siempre.
                                    if let Err(e) = client.ack(&publish) {
                                        tracing::warn!(error = %e, "failed to ack malformed publish");
                                    }
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
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }
            })
            .expect("failed to spawn MQTT consumer thread");

        let consumer = Self { rx };
        (consumer, tx)
    }

    pub fn receiver(&mut self) -> &mut mpsc::Receiver<WriteEnvelope> {
        &mut self.rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_envelope(key: &str) -> WriteEnvelope {
        WriteEnvelope {
            op: "upsert".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: key.into(),
            version: 1,
            ts: "2026-07-30T00:00:00Z".into(),
            idempotency_key: key.into(),
            ack_mode: "accepted".into(),
            payload: serde_json::json!({}),
        }
    }

    // Ejercita el mismo mecanismo (`try_send` sobre un canal acotado) que
    // usa el loop de la conexión MQTT para decidir si ackea o no — probar
    // la conexión real requeriría un broker, así que esto valida la
    // primitiva de la que depende toda la lógica de backpressure.
    #[test]
    fn try_send_succeeds_under_capacity_and_fails_when_full() {
        let (tx, rx) = mpsc::sync_channel::<WriteEnvelope>(1);

        assert!(tx.try_send(make_envelope("a")).is_ok());

        match tx.try_send(make_envelope("b")) {
            Err(mpsc::TrySendError::Full(_)) => {}
            other => panic!("expected Full, got {other:?}"),
        }

        let received = rx.recv().expect("channel should yield the first item");
        assert_eq!(received.key, "a");

        // Con espacio liberado, un try_send posterior debe volver a entrar.
        assert!(tx.try_send(make_envelope("c")).is_ok());
    }

    #[test]
    fn try_send_fails_closed_when_receiver_dropped() {
        let (tx, rx) = mpsc::sync_channel::<WriteEnvelope>(1);
        drop(rx);

        match tx.try_send(make_envelope("a")) {
            Err(mpsc::TrySendError::Disconnected(_)) => {}
            other => panic!("expected Disconnected, got {other:?}"),
        }
    }
}

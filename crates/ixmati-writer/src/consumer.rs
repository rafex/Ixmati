use ixmati_core::WriteEnvelope;
use rumqttc::{Client, Event, MqttOptions, Packet, Publish, QoS};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
/// usa `manual_acks`. Un comando solo se confirma después de que el hilo de
/// escritura termina con éxito su transacción SQLite; colocar un comando en
/// este canal sigue siendo únicamente una operación en memoria.
const DEFAULT_CHANNEL_CAPACITY: usize = 5000;

/// Handle para confirmar un Publish desde el hilo que confirmó la
/// transacción SQLite. `rumqttc::Client` es clonable y thread-safe; el
/// `Connection` continúa siendo conducido exclusivamente por el hilo MQTT.
#[derive(Clone)]
pub struct MqttAck {
    pub(crate) client: Client,
    pub(crate) publish: Publish,
}

impl MqttAck {
    pub fn new(client: Client, publish: Publish) -> Self {
        Self { client, publish }
    }

    pub fn ack(&self) -> Result<(), rumqttc::ClientError> {
        self.client.ack(&self.publish)
    }
}

/// Comando recibido pero todavía no confirmado al broker.
#[derive(Clone)]
pub struct PendingCommand {
    pub envelope: WriteEnvelope,
    pub ack: MqttAck,
}

impl PendingCommand {
    pub fn new(envelope: WriteEnvelope, ack: MqttAck) -> Self {
        Self { envelope, ack }
    }
}

pub struct MqttConsumer {
    rx: mpsc::Receiver<PendingCommand>,
    queue_depth: Arc<AtomicU64>,
}

impl MqttConsumer {
    pub fn new(
        broker: &str,
        client_id: &str,
        topic: &str,
    ) -> (Self, mpsc::SyncSender<PendingCommand>) {
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
    ) -> (Self, mpsc::SyncSender<PendingCommand>) {
        Self::with_capacity_impl(broker, client_id, topic, capacity, None)
    }

    pub fn with_capacity_for_store(
        broker: &str,
        client_id: &str,
        topic: &str,
        capacity: usize,
        store: &str,
    ) -> (Self, mpsc::SyncSender<PendingCommand>) {
        Self::with_capacity_impl(broker, client_id, topic, capacity, Some(store.to_string()))
    }

    fn with_capacity_impl(
        broker: &str,
        client_id: &str,
        topic: &str,
        capacity: usize,
        store_label: Option<String>,
    ) -> (Self, mpsc::SyncSender<PendingCommand>) {
        let (tx, rx) = mpsc::sync_channel(capacity);
        let queue_depth = Arc::new(AtomicU64::new(0));
        let queue_depth_thread = Arc::clone(&queue_depth);
        let store_label_thread = store_label.clone();
        record_queue_depth(0, store_label.as_deref());

        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);
        let mut mqtt_options = MqttOptions::new(client_id, &host, port);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));
        mqtt_options.set_manual_acks(true);
        // DEC-0055/TASK-VAL-0024: `clean_session=true` (default de rumqttc)
        // hace que Mosquitto descarte toda la cola QoS1 pendiente de esta
        // sesión en cada reconexión — confirmado como la causa de que un
        // reinicio del writer bajo sobrecarga pierda el backlog completo en
        // silencio en vez de recuperarlo. Con sesión persistente, el broker
        // retiene los mensajes QoS1 no confirmados mientras el writer está
        // desconectado y los reentrega al volver — siempre que `client_id`
        // sea estable entre reinicios (ver `main.rs`), no aleatorio.
        mqtt_options.set_clean_session(false);

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
                            if let Some(store) = store_label_thread.as_deref() {
                                crate::metrics::MQTT_CONSUMER_LAST_EVENT_UNIX_SECONDS.record(
                                    chrono::Utc::now().timestamp(),
                                    &[opentelemetry::KeyValue::new("store", store.to_string())],
                                );
                            }
                            match serde_json::from_slice::<WriteEnvelope>(&publish.payload) {
                                Ok(envelope) => {
                                    let pending = PendingCommand {
                                        envelope,
                                        ack: MqttAck {
                                            client: client.clone(),
                                            publish: publish.clone(),
                                        },
                                    };
                                    queue_depth_thread.fetch_add(1, Ordering::Relaxed);
                                    record_queue_depth(
                                        queue_depth_thread.load(Ordering::Relaxed),
                                        store_label_thread.as_deref(),
                                    );
                                    match tx_clone.try_send(pending) {
                                        Ok(()) => {
                                            crate::metrics::MQTT_COMMANDS_ENQUEUED.add(1, &[]);
                                        }
                                        Err(mpsc::TrySendError::Full(_)) => {
                                            queue_depth_thread.fetch_sub(1, Ordering::Relaxed);
                                            record_queue_depth(
                                                queue_depth_thread.load(Ordering::Relaxed),
                                                store_label_thread.as_deref(),
                                            );
                                            // El comando sigue sin ACK y permanece
                                            // pendiente en el broker. No hay
                                            // pérdida si el proceso cae.
                                            crate::metrics::MQTT_COMMANDS_DEFERRED.add(1, &[]);
                                            tracing::warn!(
                                                capacity,
                                                "consumer channel full, deferring ack (backpressure)"
                                            );
                                        }
                                        Err(mpsc::TrySendError::Disconnected(_)) => {
                                            queue_depth_thread.fetch_sub(1, Ordering::Relaxed);
                                            record_queue_depth(
                                                queue_depth_thread.load(Ordering::Relaxed),
                                                store_label_thread.as_deref(),
                                            );
                                            tracing::error!(
                                                "consumer channel closed, dropping message"
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        topic = %publish.topic,
                                        "failed to deserialize command"
                                    );
                                    // Mensaje envenenado: reintentar no lo va
                                    // a arreglar. Ackear para no
                                    // redistribuirlo por siempre.
                                    if let Err(e) = client.try_ack(&publish) {
                                        tracing::warn!(error = %e, "failed to ack malformed publish");
                                        crate::metrics::MQTT_ACK_FAILURES.add(1, &[]);
                                    }
                                }
                            }
                        }
                        Ok(Event::Incoming(Packet::ConnAck(_))) => {
                            if let Some(store) = store_label_thread.as_deref() {
                                crate::metrics::MQTT_CONSUMER_CONNECTED.record(
                                    1,
                                    &[opentelemetry::KeyValue::new("store", store.to_string())],
                                );
                            }
                            tracing::info!(
                                topic = %topic_owned,
                                "MQTT consumer connected and subscribed"
                            );
                        }
                        Ok(_) => {}
                        Err(e) => {
                            if let Some(store) = store_label_thread.as_deref() {
                                crate::metrics::MQTT_CONSUMER_CONNECTED.record(
                                    0,
                                    &[opentelemetry::KeyValue::new("store", store.to_string())],
                                );
                                crate::metrics::MQTT_EVENTLOOP_ERRORS.add(
                                    1,
                                    &[opentelemetry::KeyValue::new("store", store.to_string())],
                                );
                            }
                            tracing::error!(error = %e, "MQTT event loop error");
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }
            })
            .expect("failed to spawn MQTT consumer thread");

        let consumer = Self { rx, queue_depth };
        (consumer, tx)
    }

    pub fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<PendingCommand, mpsc::RecvTimeoutError> {
        let result = self.rx.recv_timeout(timeout);
        if result.is_ok() {
            self.queue_depth.fetch_sub(1, Ordering::Relaxed);
        }
        result
    }

    pub fn queue_depth(&self) -> u64 {
        self.queue_depth.load(Ordering::Relaxed)
    }
}

fn record_queue_depth(depth: u64, store: Option<&str>) {
    if let Some(store) = store {
        crate::metrics::CONSUMER_QUEUE_DEPTH.record(
            depth,
            &[opentelemetry::KeyValue::new("store", store.to_string())],
        );
    } else {
        crate::metrics::CONSUMER_QUEUE_DEPTH.record(depth, &[]);
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
        let (tx, rx) = mpsc::sync_channel::<PendingCommand>(1);

        let make_pending = |key: &str| PendingCommand {
            envelope: make_envelope(key),
            ack: test_ack(),
        };

        assert!(tx.try_send(make_pending("a")).is_ok());

        match tx.try_send(make_pending("b")) {
            Err(mpsc::TrySendError::Full(_)) => {}
            _ => panic!("expected Full"),
        }

        let received = rx.recv().expect("channel should yield the first item");
        assert_eq!(received.envelope.key, "a");

        // Con espacio liberado, un try_send posterior debe volver a entrar.
        assert!(tx.try_send(make_pending("c")).is_ok());
    }

    #[test]
    fn try_send_fails_closed_when_receiver_dropped() {
        let (tx, rx) = mpsc::sync_channel::<PendingCommand>(1);
        drop(rx);

        match tx.try_send(PendingCommand {
            envelope: make_envelope("a"),
            ack: test_ack(),
        }) {
            Err(mpsc::TrySendError::Disconnected(_)) => {}
            _ => panic!("expected Disconnected"),
        }
    }

    fn test_ack() -> MqttAck {
        let (client, _connection) =
            Client::new(MqttOptions::new("ixmati-test", "localhost", 1883), 10);
        MqttAck {
            client,
            publish: Publish::new("ixmati/test", QoS::AtLeastOnce, Vec::new()),
        }
    }
}

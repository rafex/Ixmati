use rumqttc::{Client, Connection, Event, MqttOptions, Outgoing, Packet, QoS};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};

use crate::outbox::Outbox;

#[derive(Default)]
struct PublishTracker {
    queued: VecDeque<i64>,
    inflight: HashMap<u16, i64>,
}

impl PublishTracker {
    fn queue(&mut self, row_id: i64) {
        self.queued.push_back(row_id);
    }

    fn remove_queued(&mut self, row_id: i64) {
        self.queued.retain(|id| *id != row_id);
    }

    fn contains_row(&self, row_id: i64) -> bool {
        self.queued.iter().any(|id| *id == row_id) || self.inflight.values().any(|id| *id == row_id)
    }

    /// `Outgoing::Publish` is emitted after rumqttc assigns the MQTT packet
    /// id. Retransmissions reuse that packet id and must not consume another
    /// outbox row from the queue.
    fn on_outgoing_publish(&mut self, packet_id: u16) {
        if packet_id == 0 || self.inflight.contains_key(&packet_id) {
            return;
        }
        if let Some(row_id) = self.queued.pop_front() {
            self.inflight.insert(packet_id, row_id);
        }
    }

    fn on_puback(&mut self, packet_id: u16) -> Option<i64> {
        self.inflight.remove(&packet_id)
    }
}

/// Publicador de eventos del outbox, sin tokio (ver DEC-0052). Antes
/// (DEC-0050/`TASK-VAL-0019`) esta función corría dentro de una `async fn`
/// y hacía I/O de SQLite síncrono directo sobre un hilo worker de tokio
/// compartido con el resto del proceso — con solo 2 hilos worker en el
/// contenedor de producción, eso podía quitarle hasta el 50% de la
/// capacidad de cómputo del runtime al resto de las tareas cada
/// `PUBLISH_INTERVAL_MS`. Ahora corre en su propio hilo de SO dedicado: no
/// hay ningún runtime async al que quitarle hilos.
pub struct EventPublisher {
    client: Client,
    db_path: String,
    store: String,
    tracker: Arc<Mutex<PublishTracker>>,
    puback_rx: Mutex<Receiver<i64>>,
    late_acks: Mutex<HashSet<i64>>,
}

impl EventPublisher {
    pub fn new(broker: &str, client_id: &str, db_path: &str, store: &str) -> Self {
        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);
        let mut mqtt_options = MqttOptions::new(format!("{}-publisher", client_id), &host, port);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));
        // DEC-0055/TASK-VAL-0024: mismo motivo que en consumer.rs — sesión
        // estable en vez de `clean_session=true` (default). Este cliente solo
        // publica (no se suscribe a nada), así que no tiene una cola QoS1
        // propia que perder, pero mantener la sesión consistente con el
        // consumidor evita sorpresas y deja la identidad MQTT del writer
        // completa (consumer + publisher) alineada con `client_id` estable.
        mqtt_options.set_clean_session(false);

        let (client, connection) = Client::new(mqtt_options, 100);
        let tracker = Arc::new(Mutex::new(PublishTracker::default()));
        let (puback_tx, puback_rx) = mpsc::channel();

        Self::spawn_eventloop(
            connection,
            Arc::clone(&tracker),
            puback_tx,
            store.to_string(),
        );

        Self {
            client,
            db_path: db_path.to_string(),
            store: store.to_string(),
            tracker,
            puback_rx: Mutex::new(puback_rx),
            late_acks: Mutex::new(HashSet::new()),
        }
    }

    /// Hilo dedicado que conduce la conexión MQTT del publicador
    /// (`rumqttc::Connection`, API síncrona). Igual que en `consumer.rs`:
    /// el runtime interno de rumqttc queda confinado a este hilo, sin
    /// cruzar hacia el resto del proceso.
    fn spawn_eventloop(
        mut connection: Connection,
        tracker: Arc<Mutex<PublishTracker>>,
        puback_tx: Sender<i64>,
        store: String,
    ) {
        std::thread::Builder::new()
            .name("ixmati-mqtt-publisher".into())
            .spawn(move || {
                for notification in connection.iter() {
                    match notification {
                        Ok(Event::Outgoing(Outgoing::Publish(packet_id))) => {
                            tracker.lock().unwrap().on_outgoing_publish(packet_id);
                        }
                        Ok(Event::Incoming(Packet::PubAck(puback))) => {
                            crate::metrics::OUTBOX_LAST_PUBACK_UNIX_SECONDS.record(
                                chrono::Utc::now().timestamp(),
                                &[opentelemetry::KeyValue::new("store", store.clone())],
                            );
                            let row_id = tracker.lock().unwrap().on_puback(puback.pkid);
                            if let Some(row_id) = row_id {
                                let _ = puback_tx.send(row_id);
                            }
                        }
                        Ok(_) => continue,
                        Err(e) => {
                            crate::metrics::MQTT_EVENTLOOP_ERRORS
                                .add(1, &[opentelemetry::KeyValue::new("store", store.clone())]);
                            tracing::error!(error = %e, "MQTT publisher eventloop error");
                            std::thread::sleep(std::time::Duration::from_secs(5));
                        }
                    }
                }
            })
            .expect("failed to spawn MQTT publisher eventloop thread");
    }

    /// Publica hasta `limit` eventos pendientes del outbox a MQTT y marca
    /// únicamente los que recibieron PUBACK del broker.
    ///
    /// La publicación se encola de forma asíncrona en rumqttc, pero el
    /// `UPDATE` durable sólo ocurre después de observar el PUBACK asociado al
    /// packet id MQTT. Si el proceso cae antes de ese punto, el evento queda
    /// en el outbox y se reintenta de forma segura.
    pub fn publish_unpublished(&self, store: &str, limit: usize) -> rusqlite::Result<usize> {
        let rows = {
            let conn = crate::db::open_with_pragmas(&self.db_path)?;
            Outbox::fetch_unpublished(&conn, store, limit)?
        };

        if rows.is_empty() {
            return Ok(0);
        }

        let mut queued_ids = HashSet::with_capacity(rows.len());
        for row in &rows {
            let topic = format!("ixmati/evt/{}/{}/{}", row.store, row.entity, row.key);
            let already_tracked = {
                let mut tracker = self.tracker.lock().unwrap();
                if tracker.contains_row(row.id) {
                    true
                } else {
                    tracker.queue(row.id);
                    false
                }
            };
            queued_ids.insert(row.id);
            if already_tracked {
                continue;
            }
            crate::metrics::OUTBOX_PUBLISH_ATTEMPTS.add(
                1,
                &[opentelemetry::KeyValue::new("store", self.store.clone())],
            );
            match self
                .client
                .publish(&topic, QoS::AtLeastOnce, false, row.payload.clone())
            {
                Ok(()) => {}
                Err(e) => {
                    self.tracker.lock().unwrap().remove_queued(row.id);
                    tracing::warn!(
                        error = %e,
                        event_id = %row.event_id,
                        store = %row.store,
                        "failed to publish event"
                    );
                }
            }
        }

        let published_ids = self.wait_for_pubacks(&queued_ids, std::time::Duration::from_secs(5));
        let timed_out = queued_ids.len().saturating_sub(published_ids.len());
        if timed_out > 0 {
            crate::metrics::OUTBOX_PUBACK_TIMEOUTS.add(
                timed_out as u64,
                &[opentelemetry::KeyValue::new("store", self.store.clone())],
            );
        }
        if !published_ids.is_empty() {
            let conn = crate::db::open_with_pragmas(&self.db_path)?;
            Outbox::mark_published_batch(&conn, &published_ids)?;
        }

        Ok(published_ids.len())
    }

    fn wait_for_pubacks(&self, expected: &HashSet<i64>, timeout: std::time::Duration) -> Vec<i64> {
        if expected.is_empty() {
            return Vec::new();
        }

        let deadline = std::time::Instant::now() + timeout;
        let mut acked = self.late_acks.lock().unwrap();
        let rx = self.puback_rx.lock().unwrap();

        loop {
            while let Ok(row_id) = rx.try_recv() {
                acked.insert(row_id);
            }

            if expected.iter().all(|row_id| acked.contains(row_id)) {
                break;
            }

            let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
                break;
            };
            match rx.recv_timeout(remaining.min(std::time::Duration::from_millis(100))) {
                Ok(row_id) => {
                    acked.insert(row_id);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        expected
            .iter()
            .filter(|row_id| acked.remove(row_id))
            .copied()
            .collect()
    }

    pub fn publish_loop(
        publisher: Arc<EventPublisher>,
        store: String,
        interval_ms: u64,
        batch_limit: usize,
    ) {
        let interval = std::time::Duration::from_millis(interval_ms);

        loop {
            std::thread::sleep(interval);

            match publisher.publish_unpublished(&store, batch_limit) {
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
    use super::PublishTracker;
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

    #[test]
    fn tracker_matches_packet_ids_and_ignores_retransmits() {
        let mut tracker = PublishTracker::default();
        tracker.queue(10);
        tracker.queue(11);

        tracker.on_outgoing_publish(7);
        tracker.on_outgoing_publish(7);
        tracker.on_outgoing_publish(8);

        assert_eq!(tracker.inflight.get(&7), Some(&10));
        assert_eq!(tracker.inflight.get(&8), Some(&11));
        assert_eq!(tracker.on_puback(7), Some(10));
        assert_eq!(tracker.on_puback(7), None);
    }
}

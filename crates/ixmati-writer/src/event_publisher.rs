use rumqttc::{Client, Connection, Event, MqttOptions, Outgoing, Packet, QoS};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

use crate::outbox::Outbox;
use crate::write_thread::WriteHandle;

#[derive(Default)]
struct PublishTracker {
    queued: VecDeque<i64>,
    inflight: HashMap<u16, i64>,
    pending_since: HashMap<i64, Instant>,
    timeout_reported: HashSet<i64>,
}

impl PublishTracker {
    fn queue(&mut self, row_id: i64) {
        self.queued.push_back(row_id);
        self.pending_since.insert(row_id, Instant::now());
        self.timeout_reported.remove(&row_id);
    }

    fn remove_queued(&mut self, row_id: i64) {
        self.queued.retain(|id| *id != row_id);
        self.pending_since.remove(&row_id);
        self.timeout_reported.remove(&row_id);
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
        let row_id = self.inflight.remove(&packet_id)?;
        self.pending_since.remove(&row_id);
        self.timeout_reported.remove(&row_id);
        Some(row_id)
    }

    fn count_new_timeouts(&mut self, timeout: Duration) -> usize {
        let mut count = 0;
        for (&row_id, since) in &self.pending_since {
            if since.elapsed() >= timeout && self.timeout_reported.insert(row_id) {
                count += 1;
            }
        }
        count
    }
}

const PUBACK_TIMEOUT: Duration = Duration::from_secs(5);
const PUBACK_CHANNEL_CAPACITY: usize = 4096;
const PENDING_PUBACK_CAPACITY: usize = 4096;

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
    write_handle: WriteHandle,
    tracker: Arc<Mutex<PublishTracker>>,
    puback_rx: Mutex<Receiver<i64>>,
    pending_pubacks: Mutex<HashSet<i64>>,
}

impl EventPublisher {
    pub fn new(
        broker: &str,
        client_id: &str,
        db_path: &str,
        store: &str,
        write_handle: WriteHandle,
    ) -> Self {
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
        let (puback_tx, puback_rx) = mpsc::sync_channel(PUBACK_CHANNEL_CAPACITY);

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
            write_handle,
            tracker,
            puback_rx: Mutex::new(puback_rx),
            pending_pubacks: Mutex::new(HashSet::new()),
        }
    }

    /// Hilo dedicado que conduce la conexión MQTT del publicador
    /// (`rumqttc::Connection`, API síncrona). Igual que en `consumer.rs`:
    /// el runtime interno de rumqttc queda confinado a este hilo, sin
    /// cruzar hacia el resto del proceso.
    fn spawn_eventloop(
        mut connection: Connection,
        tracker: Arc<Mutex<PublishTracker>>,
        puback_tx: SyncSender<i64>,
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
                            if let Some(row_id) = row_id
                                && puback_tx.try_send(row_id).is_err()
                            {
                                crate::metrics::OUTBOX_PUBACK_QUEUE_DROPS.add(
                                    1,
                                    &[opentelemetry::KeyValue::new("store", store.clone())],
                                );
                                tracing::warn!(
                                    row_id,
                                    "PUBACK queue full; outbox row will be retried"
                                );
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
    ///
    /// El ciclo es deliberadamente ACK-driven: no espera a que un lote entero
    /// reciba PUBACK. Esperar el lote completo introduce head-of-line blocking
    /// y puede consumir cinco segundos aunque sólo una publicación haya
    /// fallado. Los PUBACK se acumulan en un buffer acotado por la iteración
    /// del publicador y se marcan en SQLite en lotes pequeños.
    pub fn publish_unpublished(&self, store: &str, limit: usize) -> rusqlite::Result<usize> {
        let mark_limit = limit.max(1);
        let mut marked = self.drain_pubacks(mark_limit)?;
        let rows = {
            let conn = crate::db::open_with_pragmas(&self.db_path)?;
            Outbox::fetch_unpublished(&conn, store, limit)?
        };

        if rows.is_empty() {
            return Ok(marked);
        }

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

        // Report stalled acknowledgements without blocking the publisher. A
        // row remains tracked and will be retried according to the normal
        // outbox loop; the metric is emitted once per pending row until an
        // ACK arrives or the publish attempt is removed.
        let timed_out = self
            .tracker
            .lock()
            .unwrap()
            .count_new_timeouts(PUBACK_TIMEOUT);
        if timed_out > 0 {
            crate::metrics::OUTBOX_PUBACK_TIMEOUTS.add(
                timed_out as u64,
                &[opentelemetry::KeyValue::new("store", self.store.clone())],
            );
        }

        // A fast broker can deliver PUBACKs before this iteration returns.
        // Drain once more so those acknowledgements do not wait for the next
        // timer tick; acknowledgements arriving later are handled at the next
        // iteration without blocking this publisher thread.
        marked += self.drain_pubacks(mark_limit)?;

        Ok(marked)
    }

    /// Drena los PUBACK recibidos y marca sus filas en SQLite. Si el UPDATE
    /// falla, conserva los IDs para que la siguiente iteración pueda reintentar
    /// sin convertir un error transitorio en pérdida de estado de publicación.
    fn drain_pubacks(&self, limit: usize) -> rusqlite::Result<usize> {
        let rx = self.puback_rx.lock().unwrap();
        let mut pending = self.pending_pubacks.lock().unwrap();
        while let Ok(row_id) = rx.try_recv() {
            if pending.len() < PENDING_PUBACK_CAPACITY {
                pending.insert(row_id);
            } else {
                crate::metrics::OUTBOX_PUBACK_QUEUE_DROPS.add(
                    1,
                    &[opentelemetry::KeyValue::new("store", self.store.clone())],
                );
                tracing::warn!(
                    row_id,
                    "PUBACK pending set full; outbox row will be retried"
                );
            }
        }
        drop(rx);

        if pending.is_empty() {
            return Ok(0);
        }

        let ids = pending.iter().copied().take(limit).collect::<Vec<_>>();
        pause_after_puback_for_test(&self.store, &ids);
        let started = std::time::Instant::now();
        match self.write_handle.mark_published_batch(ids.clone()) {
            Ok(marked) => {
                for id in &ids {
                    pending.remove(id);
                }
                crate::metrics::OUTBOX_PUBLISHED.add(
                    marked as u64,
                    &[opentelemetry::KeyValue::new("store", self.store.clone())],
                );
                crate::metrics::OUTBOX_MARK_DURATION.record(
                    started.elapsed().as_secs_f64(),
                    &[opentelemetry::KeyValue::new("store", self.store.clone())],
                );
                Ok(marked)
            }
            Err(error) => {
                crate::metrics::OUTBOX_MARK_FAILURES.add(
                    1,
                    &[opentelemetry::KeyValue::new("store", self.store.clone())],
                );
                Err(error)
            }
        }
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

/// Deterministic crash barrier for the PUBACK -> `published_at` window.
///
/// This is deliberately inert unless both environment variables are present:
/// a production process cannot be paused merely because a stale path exists.
/// The manifest is renamed atomically so an external harness can safely wait
/// until every row in the batch has received PUBACK, then send SIGKILL.
fn pause_after_puback_for_test(store: &str, outbox_ids: &[i64]) {
    if std::env::var("IXMATI_TEST_MODE").as_deref() != Ok("1") {
        return;
    }
    let Ok(path) = std::env::var("IXMATI_TEST_PUBACK_BARRIER") else {
        return;
    };

    let manifest = serde_json::json!({
        "pid": std::process::id(),
        "store": store,
        "outbox_ids": outbox_ids,
        "phase": "puback_received_before_published_at",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let path = std::path::PathBuf::from(path);
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    if let Err(error) = std::fs::write(&temporary, manifest.to_string())
        .and_then(|_| std::fs::rename(&temporary, &path))
    {
        tracing::error!(error = %error, path = %path.display(), "failed to write PUBACK test barrier");
        return;
    }

    tracing::warn!(path = %path.display(), rows = outbox_ids.len(), "test barrier reached after PUBACK and before published_at");
    while path.exists() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::PublishTracker;
    use crate::outbox::Outbox;
    use ixmati_core::EventEnvelope;
    use rusqlite::Connection;
    use std::time::Duration;

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

    #[test]
    fn failed_publish_is_removed_without_waiting_for_a_puback() {
        let mut tracker = PublishTracker::default();
        tracker.queue(10);
        tracker.remove_queued(10);

        assert!(!tracker.contains_row(10));
        assert_eq!(tracker.count_new_timeouts(Duration::ZERO), 0);
    }

    #[test]
    fn timeout_is_counted_once_until_puback() {
        let mut tracker = PublishTracker::default();
        tracker.queue(10);

        assert_eq!(tracker.count_new_timeouts(Duration::ZERO), 1);
        assert_eq!(tracker.count_new_timeouts(Duration::ZERO), 0);

        tracker.on_outgoing_publish(7);
        assert_eq!(tracker.on_puback(7), Some(10));
        assert_eq!(tracker.count_new_timeouts(Duration::ZERO), 0);
    }
}

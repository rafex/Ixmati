use rumqttc::{Client, Connection, MqttOptions, QoS};
use std::sync::Arc;

use crate::outbox::Outbox;

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
}

impl EventPublisher {
    pub fn new(broker: &str, client_id: &str, db_path: &str) -> Self {
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

        Self::spawn_eventloop(connection);

        Self {
            client,
            db_path: db_path.to_string(),
        }
    }

    /// Hilo dedicado que conduce la conexión MQTT del publicador
    /// (`rumqttc::Connection`, API síncrona). Igual que en `consumer.rs`:
    /// el runtime interno de rumqttc queda confinado a este hilo, sin
    /// cruzar hacia el resto del proceso.
    fn spawn_eventloop(mut connection: Connection) {
        std::thread::Builder::new()
            .name("ixmati-mqtt-publisher".into())
            .spawn(move || {
                for notification in connection.iter() {
                    match notification {
                        Ok(_) => continue,
                        Err(e) => {
                            tracing::error!(error = %e, "MQTT publisher eventloop error");
                            std::thread::sleep(std::time::Duration::from_secs(5));
                        }
                    }
                }
            })
            .expect("failed to spawn MQTT publisher eventloop thread");
    }

    /// Publica hasta `limit` eventos pendientes del outbox a MQTT y marca
    /// los exitosos con un único UPDATE en batch.
    ///
    /// Antes: una conexión SQLite nueva + un UPDATE individual por cada fila
    /// publicada, y publicación secuencial con `.await` por evento — con QoS 1
    /// eso topeaba el drenado muy por debajo del throughput de entrada bajo
    /// carga real (ver DEC-0043). Ahora se hace un solo
    /// `UPDATE ... WHERE id IN (...)` al final; la publicación en sí ya era
    /// (y sigue siendo) solo un encolado en el canal interno de rumqttc —
    /// `client.publish()` no espera el PUBACK, así que no hace falta
    /// paralelismo explícito para eso (ver DEC-0052).
    pub fn publish_unpublished(&self, store: &str, limit: usize) -> rusqlite::Result<usize> {
        let rows = {
            let conn = crate::db::open_with_pragmas(&self.db_path)?;
            Outbox::fetch_unpublished(&conn, store, limit)?
        };

        if rows.is_empty() {
            return Ok(0);
        }

        let mut published_ids = Vec::with_capacity(rows.len());
        for row in rows {
            let topic = format!("ixmati/evt/{}/{}/{}", row.store, row.entity, row.key);
            match self.client.publish(&topic, QoS::AtLeastOnce, false, row.payload.clone()) {
                Ok(()) => published_ids.push(row.id),
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

        let published = published_ids.len();
        if !published_ids.is_empty() {
            let conn = crate::db::open_with_pragmas(&self.db_path)?;
            Outbox::mark_published_batch(&conn, &published_ids)?;
        }

        Ok(published)
    }

    pub fn publish_loop(publisher: Arc<EventPublisher>, store: String, interval_ms: u64, batch_limit: usize) {
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

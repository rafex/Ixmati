use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::sync::Arc;

use crate::outbox::{Outbox, OutboxRow};

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
        // DEC-0052/TASK-VAL-0019: `open_with_pragmas` + `fetch_unpublished` son
        // síncronos/bloqueantes (I/O de archivo + query). Antes se llamaban
        // directo dentro de esta `async fn`, sobre uno de los hilos worker de
        // tokio — con solo 2 hilos worker en el contenedor de producción
        // (2 vCPUs), esto le quitaba hasta el 50% de la capacidad de cómputo
        // del runtime al resto de las tareas (loop principal, eventloops MQTT)
        // cada `PUBLISH_INTERVAL_MS` (200ms por defecto). Mismo patrón que
        // `process_batch` (Opción H, DEC-0050).
        let db_path = self.db_path.clone();
        let store_owned = store.to_string();
        let rows = tokio::task::spawn_blocking(move || -> rusqlite::Result<Vec<OutboxRow>> {
            let conn = crate::db::open_with_pragmas(&db_path)?;
            Outbox::fetch_unpublished(&conn, &store_owned, limit)
        })
        .await
        .expect("fetch_unpublished blocking task panicked")?;

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
            let db_path = self.db_path.clone();
            tokio::task::spawn_blocking(move || -> rusqlite::Result<()> {
                let conn = crate::db::open_with_pragmas(&db_path)?;
                Outbox::mark_published_batch(&conn, &published_ids)
            })
            .await
            .expect("mark_published_batch blocking task panicked")?;
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

    // DEC-0052: demuestra, con una prueba unitaria pura (sin Debian, sin
    // carga real), el mecanismo detrás del bug que tenía `publish_unpublished`
    // antes del fix de arriba — una llamada síncrona/bloqueante ejecutada
    // directo dentro de una `async fn`, sin `spawn_blocking`, le quita el
    // hilo worker de tokio a cualquier otra tarea del runtime mientras dura.
    //
    // Se usa `worker_threads = 1` a propósito: aísla el mecanismo de forma
    // determinística (sin depender de si el work-stealing del scheduler
    // decide mover la tarea del "ticker" a un hilo libre o no). Reproduce el
    // instante en producción (2 hilos worker) en el que ambos ya están
    // ocupados con otras tareas del pipeline (eventloops MQTT, loop
    // principal) y la llamada síncrona efectivamente tiene el runtime para
    // sí sola — no es una prueba de timing exacto, es una prueba de
    // mecanismo con umbrales generosos.
    mod worker_starvation {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        const BLOCK_DURATION: Duration = Duration::from_millis(300);
        const TICK_INTERVAL: Duration = Duration::from_millis(1);

        async fn spawn_ticker() -> (tokio::task::JoinHandle<()>, Arc<AtomicU64>) {
            let ticks = Arc::new(AtomicU64::new(0));
            let ticks_clone = Arc::clone(&ticks);
            let handle = tokio::spawn(async move {
                loop {
                    ticks_clone.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(TICK_INTERVAL).await;
                }
            });
            // Deja que el ticker arranque antes de medir.
            tokio::time::sleep(Duration::from_millis(20)).await;
            (handle, ticks)
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
        async fn sync_call_without_spawn_blocking_freezes_other_tasks() {
            let (ticker, ticks) = spawn_ticker().await;
            let before = ticks.load(Ordering::Relaxed);

            // Reproduce el patrón que tenía `publish_unpublished` antes del
            // fix: I/O síncrono/bloqueante directo dentro de una `async fn`.
            // Se lanza vía `tokio::spawn` (igual que `publish_loop` en
            // `main.rs`) para que corra en el pool de hilos worker — llamarla
            // directo desde el cuerpo de un `#[tokio::test]` no sirve: ese
            // cuerpo se ejecuta en el hilo que llama a `block_on`, que es
            // distinto del pool de workers usado por las tareas `spawn`eadas
            // incluso con `worker_threads = 1`.
            let blocker = tokio::spawn(async {
                std::thread::sleep(BLOCK_DURATION);
            });
            blocker.await.unwrap();

            let ticks_during_block = ticks.load(Ordering::Relaxed) - before;
            ticker.abort();

            // Umbral generoso (<=3 de ~300 ticks esperados sin bloqueo): el
            // scheduler puede alcanzar a darle un tick de cortesía al ticker
            // justo en el instante en que se agenda la tarea bloqueante,
            // antes de que monopolice el único hilo worker — eso es ruido de
            // orden de scheduling, no invalida el mecanismo que se prueba.
            assert!(
                ticks_during_block <= 3,
                "con 1 solo hilo worker, una llamada síncrona sin spawn_blocking \
                 debería congelar casi por completo cualquier otra tarea del \
                 runtime mientras dura — el ticker avanzó {ticks_during_block} ticks"
            );
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
        async fn sync_call_wrapped_in_spawn_blocking_does_not_freeze_other_tasks() {
            let (ticker, ticks) = spawn_ticker().await;
            let before = ticks.load(Ordering::Relaxed);

            // Mismo trabajo síncrono, pero aislado correctamente con
            // spawn_blocking (el fix aplicado en `publish_unpublished`) — corre
            // en el pool de hilos bloqueantes de tokio, no en el único hilo
            // worker, así que el resto del runtime sigue progresando.
            let blocker = tokio::spawn(async {
                tokio::task::spawn_blocking(|| std::thread::sleep(BLOCK_DURATION))
                    .await
                    .unwrap();
            });
            blocker.await.unwrap();

            let ticks_during_block = ticks.load(Ordering::Relaxed) - before;
            ticker.abort();

            assert!(
                ticks_during_block > 100,
                "se esperaba que el ticker siguiera progresando con normalidad \
                 (>100 ticks en {BLOCK_DURATION:?}) mientras el trabajo \
                 bloqueante corría en spawn_blocking, pero solo avanzó \
                 {ticks_during_block} ticks"
            );
        }
    }
}

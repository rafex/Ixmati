use ixmati_cache::SyncCacheClient;
use ixmati_writer::batcher::Batcher;
use ixmati_writer::cache_sync::CacheSync;
use ixmati_writer::consumer::MqttConsumer;
use ixmati_writer::event_publisher::EventPublisher;
use ixmati_writer::write_thread::WriteHandle;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;

/// DEC-0052: motor de escritura sin tokio. `fn main()` normal, sin
/// `#[tokio::main]` — no hay ningún runtime async compartido en el
/// proceso. Cada componente que necesita I/O concurrente (MQTT, el
/// servidor de métricas) corre en su propio hilo de SO dedicado, con su
/// propio mecanismo interno si lo necesita (rumqttc trae su propio runtime
/// de tokio confinado a un solo hilo; el servidor de métricas hace lo
/// mismo, ver `metrics.rs`) — pero ninguno de esos runtimes cruza jamás
/// hacia el camino de escritura a SQLite. La comunicación entre hilos usa
/// únicamente primitivas de `std::sync` (mpsc), nunca `tokio::sync`.
///
/// Esto resuelve de raíz la clase de bug de DEC-0050 (`process_batch`) y
/// DEC-0052 (`event_publisher.rs`): código síncrono/bloqueante llamado
/// directo dentro de una `async fn`, compitiendo por los mismos hilos
/// worker de tokio que el resto del pipeline — no puede volver a ocurrir
/// porque no existe ningún hilo worker de tokio compartido. También evita
/// el modo de falla del `WriteActor` de DEC-0051: la comunicación con el
/// hilo de escritura (`write_thread.rs`) usa `std::sync::mpsc` puro en
/// ambas direcciones, nunca `tokio::sync::oneshot`.
fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_writer=info".into()),
        )
        .json()
        .init();

    ixmati_writer::metrics::maybe_spawn_server();

    let mqtt_broker =
        std::env::var("MQTT_BROKER").unwrap_or_else(|_| "tcp://localhost:1883".into());
    let store_name = std::env::var("STORE_NAME").unwrap_or_else(|_| "pedidos".into());
    let db_path = std::env::var("SQLITE_PATH").unwrap_or_else(|_| "/data/pedidos.db".into());
    let batch_size: usize = std::env::var("BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let batch_interval_ms: u64 = std::env::var("BATCH_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let publish_interval_ms: u64 = std::env::var("PUBLISH_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let publish_batch_limit: usize = std::env::var("PUBLISH_BATCH_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    // Ver DEC-0048/TASK-VAL-0016: antes era un mpsc::unbounded_channel(),
    // causa confirmada de OOM-kill del writer bajo carga sostenida real.
    let consumer_channel_capacity: usize = std::env::var("CONSUMER_CHANNEL_CAPACITY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000);
    // DEC-0055/TASK-VAL-0024: antes un UUID aleatorio en cada arranque — con
    // `clean_session=false` (ver consumer.rs/event_publisher.rs) hace falta
    // un id ESTABLE para que Mosquitto reconozca un reinicio como la misma
    // sesión y reentregue el backlog QoS1 pendiente en vez de descartarlo.
    // Un `client_id` aleatorio nuevo cada vez era, en la práctica, siempre
    // "otro cliente" para el broker — la causa raíz de la pérdida de
    // backlog confirmada en DEC-0055.
    let client_id = format!("ixmati-writer-{}", store_name);

    tracing::info!(
        store = %store_name,
        db = %db_path,
        broker = %mqtt_broker,
        batch_size,
        batch_interval_ms,
        publish_interval_ms,
        publish_batch_limit,
        consumer_channel_capacity,
        "ixmati-writer starting"
    );

    {
        let conn = ixmati_writer::db::open_with_pragmas(&db_path)
            .map_err(|e| std::io::Error::other(format!("SQLite: {}", e)))?;
        ixmati_writer::write_engine::WriteEngine::ensure_schema(&conn, &store_name)
            .map_err(|e| std::io::Error::other(format!("Schema: {}", e)))?;
    }

    let topic = format!("ixmati/cmd/{}/#", store_name);

    let (mut consumer, _tx) =
        MqttConsumer::with_capacity(&mqtt_broker, &client_id, &topic, consumer_channel_capacity);
    let mut batcher = Batcher::new(batch_size, batch_interval_ms);

    let publisher = Arc::new(EventPublisher::new(&mqtt_broker, &client_id, &db_path));
    {
        let publisher = Arc::clone(&publisher);
        let store_clone = store_name.clone();
        std::thread::Builder::new()
            .name("ixmati-publish-loop".into())
            .spawn(move || {
                EventPublisher::publish_loop(publisher, store_clone, publish_interval_ms, publish_batch_limit);
            })
            .expect("failed to spawn publish loop thread");
    }

    let cache_socket_path = std::env::var("CACHE_SOCKET_PATH")
        .unwrap_or_else(|_| "/var/run/ixmati/cache.sock".into());

    let cache_client = Arc::new(SyncCacheClient::connect(&cache_socket_path).map_err(|e| {
        tracing::error!(error = %e, socket = %cache_socket_path, "failed to connect to cache-server");
        std::io::Error::other(format!("SyncCacheClient connect: {e}"))
    })?);

    let cache_sync = Arc::new(CacheSync::new(cache_client));

    let write_handle = WriteHandle::spawn(db_path.clone());

    // Cada iteración espera un comando hasta `batch_interval_ms` — mismo rol
    // que el `flush_interval`/`select!` de la versión con tokio, pero con
    // `recv_timeout` bloqueante sobre un hilo de SO normal.
    let flush_check = std::time::Duration::from_millis(batch_interval_ms.max(1));

    // Mapeo de puntos calientes: aísla el tiempo de "llenar" un batch bajo
    // carga real, separado del tiempo de procesarlo.
    let mut fill_started = std::time::Instant::now();

    loop {
        match consumer.receiver().recv_timeout(flush_check) {
            Ok(cmd) => {
                tracing::debug!(
                    store = %cmd.store,
                    entity = %cmd.entity,
                    key = %cmd.key,
                    version = cmd.version,
                    "received command"
                );

                if let Some(batch) = batcher.push(cmd) {
                    ixmati_writer::metrics::BATCH_FILL_DURATION.record(
                        fill_started.elapsed().as_secs_f64(),
                        &[opentelemetry::KeyValue::new("store", store_name.clone())],
                    );
                    process_batch(&write_handle, &batch, &store_name, &cache_sync);
                    fill_started = std::time::Instant::now();
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if batcher.should_flush() && !batcher.is_empty() {
                    ixmati_writer::metrics::BATCH_FILL_DURATION.record(
                        fill_started.elapsed().as_secs_f64(),
                        &[opentelemetry::KeyValue::new("store", store_name.clone())],
                    );
                    let batch = batcher.flush();
                    process_batch(&write_handle, &batch, &store_name, &cache_sync);
                    fill_started = std::time::Instant::now();
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                tracing::warn!("MQTT consumer channel closed");
                break;
            }
        }
    }

    Ok(())
}

fn process_batch(
    write_handle: &WriteHandle,
    batch: &ixmati_writer::Batch,
    store_name: &str,
    cache_sync: &Arc<CacheSync>,
) {
    let started = std::time::Instant::now();
    let result = write_handle.process_batch(batch.commands.clone());
    ixmati_writer::metrics::WRITE_BATCH_DURATION.record(
        started.elapsed().as_secs_f64(),
        &[opentelemetry::KeyValue::new("store", store_name.to_string())],
    );

    match result {
        Ok(result) => {
            let cache_sync_started = std::time::Instant::now();
            cache_sync.sync_batch(
                &result
                    .events
                    .iter()
                    .map(|e| ixmati_core::WriteEnvelope {
                        op: "upsert".into(),
                        store: e.store.clone(),
                        entity: e.entity.clone(),
                        key: e.key.clone(),
                        version: e.version,
                        ts: e.occurred_at.clone(),
                        idempotency_key: String::new(),
                        ack_mode: "accepted".into(),
                        payload: e.payload.clone(),
                    })
                    .collect::<Vec<_>>(),
            );
            ixmati_writer::metrics::CACHE_SYNC_DURATION.record(
                cache_sync_started.elapsed().as_secs_f64(),
                &[opentelemetry::KeyValue::new("store", store_name.to_string())],
            );

            tracing::info!(
                store = %store_name,
                committed = result.committed,
                duplicates = result.duplicates,
                version_conflicts = result.version_conflicts,
                events = result.events.len(),
                "batch processed"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, store = %store_name, "batch failed");
        }
    }
}

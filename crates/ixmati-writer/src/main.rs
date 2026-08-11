use ixmati_cache::SyncCacheClient;
use ixmati_writer::batcher::Batcher;
use ixmati_writer::cache_sync::CacheSync;
use ixmati_writer::consumer::MqttConsumer;
use ixmati_writer::event_publisher::EventPublisher;
use ixmati_writer::write_thread::WriteHandle;
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

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

    let (mut consumer, _tx) = MqttConsumer::with_capacity_for_store(
        &mqtt_broker,
        &client_id,
        &topic,
        consumer_channel_capacity,
        &store_name,
    );
    let watchdog_timeout = std::env::var("MQTT_WATCHDOG_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_default();
    let progress = Arc::new(ixmati_writer::watchdog::Progress::default());
    ixmati_writer::watchdog::spawn(Arc::clone(&progress), watchdog_timeout);
    let mut batcher = Batcher::new(batch_size, batch_interval_ms);

    let publisher = Arc::new(EventPublisher::new(
        &mqtt_broker,
        &client_id,
        &db_path,
        &store_name,
    ));
    {
        let publisher = Arc::clone(&publisher);
        let store_clone = store_name.clone();
        std::thread::Builder::new()
            .name("ixmati-publish-loop".into())
            .spawn(move || {
                EventPublisher::publish_loop(
                    publisher,
                    store_clone,
                    publish_interval_ms,
                    publish_batch_limit,
                );
            })
            .expect("failed to spawn publish loop thread");
    }

    let cache_socket_path =
        std::env::var("CACHE_SOCKET_PATH").unwrap_or_else(|_| "/var/run/ixmati/cache.sock".into());

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
        match consumer.recv_timeout(flush_check) {
            Ok(cmd) => {
                progress.command_received();
                tracing::debug!(
                    store = %cmd.envelope.store,
                    entity = %cmd.envelope.entity,
                    key = %cmd.envelope.key,
                    version = cmd.envelope.version,
                    "received command"
                );

                ixmati_writer::metrics::CONSUMER_QUEUE_DEPTH.record(
                    consumer.queue_depth(),
                    &[opentelemetry::KeyValue::new("store", store_name.clone())],
                );

                if let Some(batch) = batcher.push(cmd) {
                    ixmati_writer::metrics::BATCH_FILL_DURATION.record(
                        fill_started.elapsed().as_secs_f64(),
                        &[opentelemetry::KeyValue::new("store", store_name.clone())],
                    );
                    if process_batch(&write_handle, &batch, &store_name, &cache_sync) {
                        progress.batch_committed();
                    }
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
                    if process_batch(&write_handle, &batch, &store_name, &cache_sync) {
                        progress.batch_committed();
                    }
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
) -> bool {
    let started = std::time::Instant::now();
    let envelopes = batch
        .commands
        .iter()
        .map(|pending| pending.envelope.clone())
        .collect::<Vec<_>>();
    let result = write_handle.process_batch(envelopes);
    ixmati_writer::metrics::WRITE_BATCH_DURATION.record(
        started.elapsed().as_secs_f64(),
        &[opentelemetry::KeyValue::new(
            "store",
            store_name.to_string(),
        )],
    );

    match result {
        Ok(result) => {
            ixmati_writer::metrics::WRITE_THREAD_QUEUE_WAIT.record(
                result.write_queue_wait_seconds,
                &[opentelemetry::KeyValue::new(
                    "store",
                    store_name.to_string(),
                )],
            );
            ixmati_writer::metrics::SQLITE_PROCESS_DURATION.record(
                result.sqlite_process_seconds,
                &[opentelemetry::KeyValue::new(
                    "store",
                    store_name.to_string(),
                )],
            );
            // El commit SQLite es la frontera de durabilidad. El ACK se envía
            // antes de tocar el cache porque el cache es una proyección
            // reconstruible; nunca debe convertirse en una causa de pérdida
            // o de reentrega indefinida de comandos ya persistidos.
            let ack_started = std::time::Instant::now();
            for pending in &batch.commands {
                match pending.ack.ack() {
                    Ok(()) => {
                        ixmati_writer::metrics::MQTT_COMMANDS_ACKED.add(1, &[]);
                        ixmati_writer::metrics::MQTT_CONSUMER_LAST_ACK_UNIX_SECONDS.record(
                            chrono::Utc::now().timestamp(),
                            &[opentelemetry::KeyValue::new(
                                "store",
                                store_name.to_string(),
                            )],
                        );
                    }
                    Err(e) => {
                        ixmati_writer::metrics::MQTT_ACK_FAILURES.add(1, &[]);
                        tracing::warn!(error = %e, "failed to ack committed MQTT command")
                    }
                }
            }
            ixmati_writer::metrics::BATCH_ACK_DURATION.record(
                ack_started.elapsed().as_secs_f64(),
                &[opentelemetry::KeyValue::new(
                    "store",
                    store_name.to_string(),
                )],
            );
            ixmati_writer::metrics::LAST_BATCH_COMMIT_UNIX_SECONDS.record(
                chrono::Utc::now().timestamp(),
                &[opentelemetry::KeyValue::new(
                    "store",
                    store_name.to_string(),
                )],
            );

            let cache_sync_started = std::time::Instant::now();
            let cache_errors = cache_sync.sync_batch(
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
            if cache_errors > 0 {
                ixmati_writer::metrics::CACHE_SYNC_ERRORS.add(
                    cache_errors as u64,
                    &[opentelemetry::KeyValue::new(
                        "store",
                        store_name.to_string(),
                    )],
                );
            }
            ixmati_writer::metrics::CACHE_SYNC_DURATION.record(
                cache_sync_started.elapsed().as_secs_f64(),
                &[opentelemetry::KeyValue::new(
                    "store",
                    store_name.to_string(),
                )],
            );

            tracing::info!(
                store = %store_name,
                committed = result.committed,
                duplicates = result.duplicates,
                version_conflicts = result.version_conflicts,
                events = result.events.len(),
                "batch processed"
            );
            ixmati_writer::metrics::BATCH_CYCLE_DURATION.record(
                started.elapsed().as_secs_f64(),
                &[opentelemetry::KeyValue::new(
                    "store",
                    store_name.to_string(),
                )],
            );
            true
        }
        Err(e) => {
            tracing::error!(error = %e, store = %store_name, "batch failed");
            false
        }
    }
}

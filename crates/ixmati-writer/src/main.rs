use ixmati_cache::CacheClient;
use ixmati_writer::batcher::Batcher;
use ixmati_writer::cache_sync::CacheSync;
use ixmati_writer::consumer::MqttConsumer;
use ixmati_writer::event_publisher::EventPublisher;
use ixmati_writer::write_engine::WriteEngine;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> std::io::Result<()> {
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
    // Antes hardcodeado a 1000ms/100 eventos en el propio código (ver
    // DEC-0043): a 448 writes/s de entrada esto topeaba el drenado del
    // outbox a ~100 eventos/s y generaba un backlog sin límite. Ahora
    // configurable, y publish_unpublished publica en paralelo en vez de
    // secuencial, así que el límite ya no es un techo de "1 por await".
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
    let client_id = format!("writer-{}", uuid::Uuid::new_v4());

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
        WriteEngine::ensure_schema(&conn, &store_name)
            .map_err(|e| std::io::Error::other(format!("Schema: {}", e)))?;
    }

    let topic = format!("ixmati/cmd/{}/#", store_name);

    let (mut consumer, _tx) =
        MqttConsumer::with_capacity(&mqtt_broker, &client_id, &topic, consumer_channel_capacity);
    let mut batcher = Batcher::new(batch_size, batch_interval_ms);
    let publisher = Arc::new(EventPublisher::new(&mqtt_broker, &client_id, &db_path));
    let publisher_clone = Arc::clone(&publisher);
    let store_clone = store_name.clone();

    tokio::spawn(async move {
        EventPublisher::publish_loop(
            publisher_clone,
            store_clone,
            publish_interval_ms,
            publish_batch_limit,
        )
        .await;
    });

    let cache_socket_path = std::env::var("CACHE_SOCKET_PATH")
        .unwrap_or_else(|_| "/var/run/ixmati/cache.sock".into());

    let cache_client = Arc::new(
        CacheClient::connect(&cache_socket_path)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, socket = %cache_socket_path, "failed to connect to cache-server");
                std::io::Error::other(format!("CacheClient connect: {e}"))
            })?,
    );

    let cache_sync = Arc::new(CacheSync::new(Arc::clone(&cache_client)));

    let conn = Arc::new(Mutex::new(
        ixmati_writer::db::open_with_pragmas(&db_path)
            .map_err(|e| std::io::Error::other(format!("SQLite: {}", e)))?,
    ));

    // DEC-0050/Opción G: antes se creaba un `tokio::time::sleep(50ms)` nuevo
    // en CADA iteración del loop, incluso aunque casi siempre pierde la
    // carrera contra `recv()`. Registrar/desregistrar un timer en el driver
    // de tokio tiene overhead real; para llenar un solo batch de 100
    // comandos hacen falta 100 iteraciones, cada una pagando ese costo.
    // Medido: con A (`synchronous=NORMAL`), F (`cache_sync` concurrente) y
    // hasta triplicar CPU o cambiar de entorno por completo (Mac emulado →
    // Debian real 8 cores nativo), el throughput se mantuvo fijo en
    // ~30-34 commits/s — la firma de un costo por iteración, no de
    // saturación de recursos. Un `interval` creado una sola vez fuera del
    // loop reutiliza el mismo timer en cada `tick()` en vez de registrar
    // uno nuevo por vuelta.
    let mut flush_interval = tokio::time::interval(tokio::time::Duration::from_millis(50));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Mapeo de puntos calientes: aísla el tiempo de "llenar" un batch (las
    // hasta 100 llamadas a `recv()` vía select!) bajo carga real, separado
    // del tiempo de procesarlo (WRITE_BATCH_DURATION/CACHE_SYNC_DURATION).
    let mut fill_started = std::time::Instant::now();

    loop {
        tokio::select! {
            cmd = consumer.receiver().recv() => {
                match cmd {
                    Some(cmd) => {
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
                            process_batch(&conn, &batch, &store_name, &cache_sync).await;
                            fill_started = std::time::Instant::now();
                        }
                    }
                    None => {
                        tracing::warn!("MQTT consumer channel closed");
                        break;
                    }
                }
            }
            _ = flush_interval.tick() => {
                if batcher.should_flush() && !batcher.is_empty() {
                    ixmati_writer::metrics::BATCH_FILL_DURATION.record(
                        fill_started.elapsed().as_secs_f64(),
                        &[opentelemetry::KeyValue::new("store", store_name.clone())],
                    );
                    let batch = batcher.flush();
                    process_batch(&conn, &batch, &store_name, &cache_sync).await;
                    fill_started = std::time::Instant::now();
                }
            }
        }
    }

    Ok(())
}

async fn process_batch(
    conn: &Arc<Mutex<Connection>>,
    batch: &ixmati_writer::Batch,
    store_name: &str,
    cache_sync: &Arc<CacheSync>,
) {
    // Opción H (mapeo de puntos calientes): `WriteEngine::process_batch` es
    // síncrono/bloqueante y antes se llamaba directo dentro de esta `async
    // fn`, sobre un hilo del runtime de tokio — el mismo patrón que ya se
    // había identificado y corregido en `cache_server.rs` (DEC-0045). Los
    // benchmarks aislados (`examples/bench_disk.rs`) descartaron que SQLite
    // en sí sea el cuello de botella (3900+ writes/s incluso en el disco
    // más lento del contenedor, contra ~30-35 commits/s del sistema
    // completo en vivo) — la sospecha recae en la contención por hilos del
    // runtime entre esta llamada bloqueante y las hasta 100 tareas
    // concurrentes que `cache_sync.sync_batch` lanza por batch.
    // `blocking_lock()` es la forma correcta de tomar un `tokio::sync::
    // Mutex` desde dentro de una closure no-async ejecutada en
    // `spawn_blocking`.
    let conn = Arc::clone(conn);
    let cmds = batch.commands.clone();

    let started = std::time::Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        let mut guard = conn.blocking_lock();
        WriteEngine::process_batch(&mut guard, &cmds)
    })
    .await
    .expect("process_batch blocking task panicked");
    ixmati_writer::metrics::WRITE_BATCH_DURATION.record(
        started.elapsed().as_secs_f64(),
        &[opentelemetry::KeyValue::new("store", store_name.to_string())],
    );

    match result {
        Ok(result) => {
            let cache_sync_started = std::time::Instant::now();
            cache_sync.sync_batch(&result.events
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
                .collect::<Vec<_>>())
                .await;
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

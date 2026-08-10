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
    let client_id = format!("writer-{}", uuid::Uuid::new_v4());

    tracing::info!(
        store = %store_name,
        db = %db_path,
        broker = %mqtt_broker,
        batch_size,
        batch_interval_ms,
        publish_interval_ms,
        publish_batch_limit,
        "ixmati-writer starting"
    );

    {
        let conn = Connection::open(&db_path)
            .map_err(|e| std::io::Error::other(format!("SQLite: {}", e)))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();
        WriteEngine::ensure_schema(&conn, &store_name)
            .map_err(|e| std::io::Error::other(format!("Schema: {}", e)))?;
    }

    let topic = format!("ixmati/cmd/{}/#", store_name);

    let (mut consumer, _tx) = MqttConsumer::new(&mqtt_broker, &client_id, &topic);
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
        Connection::open(&db_path).map_err(|e| std::io::Error::other(format!("SQLite: {}", e)))?,
    ));

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
                            process_batch(&conn, &batch, &store_name, &cache_sync).await;
                        }
                    }
                    None => {
                        tracing::warn!("MQTT consumer channel closed");
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {
                if batcher.should_flush() && !batcher.is_empty() {
                    let batch = batcher.flush();
                    process_batch(&conn, &batch, &store_name, &cache_sync).await;
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
    let cmds = &batch.commands;
    let mut guard = conn.lock().await;

    match WriteEngine::process_batch(&mut guard, cmds) {
        Ok(result) => {
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
                .collect::<Vec<_>>());

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

use ixmati_core::mqtt::parse_mqtt_broker;
use ixmati_writer::batcher::Batcher;
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

    let mqtt_broker = std::env::var("MQTT_BROKER").unwrap_or_else(|_| "tcp://localhost:1883".into());
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
    let client_id = format!("writer-{}", uuid::Uuid::new_v4());

    tracing::info!(
        store = %store_name,
        db = %db_path,
        broker = %mqtt_broker,
        batch_size,
        batch_interval_ms,
        "ixmati-writer starting"
    );

    {
        let mut conn = Connection::open(&db_path).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("SQLite: {}", e))
        })?;
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();
        WriteEngine::ensure_schema(&conn, &store_name).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Schema: {}", e))
        })?;
    }

    let topic = format!("ixmati/cmd/{}/#", store_name);

    let (mut consumer, _tx) = MqttConsumer::new(&mqtt_broker, &client_id, &topic);
    let mut batcher = Batcher::new(batch_size, batch_interval_ms);
    let publisher = Arc::new(EventPublisher::new(&mqtt_broker, &client_id, &db_path));
    let publisher_clone = Arc::clone(&publisher);
    let store_clone = store_name.clone();

    tokio::spawn(async move {
        EventPublisher::publish_loop(publisher_clone, store_clone, 1000).await;
    });

    let conn = Arc::new(Mutex::new(
        Connection::open(&db_path).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("SQLite: {}", e))
        })?,
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
                            process_batch(&conn, &batch, &store_name, &db_path).await;
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
                    process_batch(&conn, &batch, &store_name, &db_path).await;
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
    _db_path: &str,
) {
    let cmds = &batch.commands;
    let mut guard = conn.lock().await;

    match WriteEngine::process_batch(&mut guard, cmds) {
        Ok(result) => {
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

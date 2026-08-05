use ixmati_cache::CacheBackend;
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
        EventPublisher::publish_loop(publisher_clone, store_clone, 1000).await;
    });

    let backend = std::env::var("CACHE_BACKEND").unwrap_or_default();
    let cache: Arc<dyn CacheBackend> = if let Ok(dir) = std::env::var("CACHE_DIR") {
        match backend.as_str() {
            #[cfg(feature = "flashdb")]
            "flashdb" => match ixmati_cache::FlashDb::new(&dir) {
                Ok(fdb) => {
                    tracing::info!(cache_dir = %dir, "FlashDB initialized for writer");
                    Arc::new(fdb)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "FlashDB init failed, using NoOp");
                    Arc::new(ixmati_cache::NoOpBackend::new())
                }
            },
            "redb" => {
                let path = format!("{}/cache.redb", dir);
                match ixmati_cache::RedbCacheBackend::new(&path) {
                    Ok(backend) => {
                        tracing::info!(cache_path = %path, "RedbCacheBackend initialized for writer");
                        Arc::new(backend)
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Redb init failed, using NoOp");
                        Arc::new(ixmati_cache::NoOpBackend::new())
                    }
                }
            }
            "sqlite" => {
                let path = format!("{}/cache.db", dir);
                match ixmati_cache::SqliteCacheBackend::new(&path) {
                    Ok(backend) => {
                        tracing::info!(cache_path = %path, "SqliteCacheBackend initialized for writer");
                        Arc::new(backend)
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "SqliteCache init failed, using NoOp");
                        Arc::new(ixmati_cache::NoOpBackend::new())
                    }
                }
            }
            _ => {
                tracing::info!(backend = %backend, "unknown CACHE_BACKEND, using NoOp");
                Arc::new(ixmati_cache::NoOpBackend::new())
            }
        }
    } else {
        Arc::new(ixmati_cache::NoOpBackend::new())
    };

    let cache_sync = Arc::new(CacheSync::new(Arc::clone(&cache)));

    let cache_read_mode = std::env::var("CACHE_READ_MODE").unwrap_or_default();
    match cache_read_mode.as_str() {
        "mqtt" => {
            let _responder = ixmati_writer::cache_responder::CacheResponder::new(
                Arc::clone(&cache),
                &mqtt_broker,
            );
            tracing::info!("CacheResponder started (MQTT mode)");
        }
        "socket" => {
            let socket_path = std::env::var("CACHE_SOCKET_PATH")
                .unwrap_or_else(|_| "/var/run/ixmati/cache.sock".into());
            let server = ixmati_writer::cache_server::CacheServer::new(
                Arc::clone(&cache),
                &socket_path,
            );
            tokio::spawn(async move {
                if let Err(e) = server.run().await {
                    tracing::error!(error = %e, "CacheServer failed");
                }
            });
            tracing::info!(path = %socket_path, "CacheServer started (socket mode)");
        }
        _ => {}
    }

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
                            process_batch(&conn, &batch, &store_name, &db_path, &cache_sync).await;
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
                    process_batch(&conn, &batch, &store_name, &db_path, &cache_sync).await;
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

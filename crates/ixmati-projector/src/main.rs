use ixmati_cache::CacheClient;
use ixmati_core::{EventEnvelope, ProjectionConfig, ProjectionRegistry};
use ixmati_projector::ProjectorEngine;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(serde::Deserialize)]
struct ProjectionsFile {
    projections: Vec<ProjectionConfig>,
}

fn load_projections(path: &str) -> ProjectionRegistry {
    match std::fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<ProjectionsFile>(&content) {
            Ok(file) => {
                let registry = ProjectionRegistry::new(file.projections);
                match registry.validate() {
                    Ok(()) => {
                        tracing::info!(count = registry.len(), path = %path, "projections loaded");
                    }
                    Err(errors) => {
                        tracing::error!(?errors, "projection validation errors");
                        std::process::exit(1);
                    }
                }
                registry
            }
            Err(e) => {
                tracing::error!(error = %e, path = %path, "failed to parse projections file");
                std::process::exit(1);
            }
        },
        Err(e) => {
            tracing::error!(error = %e, path = %path, "failed to read projections file");
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_projector=info,rumqttc=warn".into()),
        )
        .json()
        .init();

    ixmati_projector::metrics::maybe_spawn_server();

    let mqtt_broker =
        std::env::var("MQTT_BROKER").unwrap_or_else(|_| "tcp://localhost:1883".into());
    let socket_path =
        std::env::var("CACHE_SOCKET_PATH").unwrap_or_else(|_| "/var/run/ixmati/cache.sock".into());
    let projections_path = std::env::var("IXMATI_PROJECTIONS_PATH")
        .unwrap_or_else(|_| "config/projections.toml".into());
    let dedup_capacity: usize = std::env::var("IXMATI_DEDUP_CAPACITY")
        .unwrap_or_else(|_| "100000".into())
        .parse()
        .unwrap_or(100_000);

    tracing::info!(
        broker = %mqtt_broker,
        socket = %socket_path,
        projections = %projections_path,
        dedup_capacity,
        "ixmati-projector starting"
    );

    let registry = load_projections(&projections_path);

    let cache = match CacheClient::connect(&socket_path).await {
        Ok(c) => Arc::new(c),
        Err(e) => {
            tracing::error!(error = %e, socket = %socket_path, "failed to connect to cache-server");
            std::process::exit(1);
        }
    };

    let engine = ProjectorEngine::new(registry, Arc::new(ixmati_cache::NoOpBackend::new()));

    let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(&mqtt_broker);
    let mut mqtt_options = MqttOptions::new("ixmati-projector", &host, port);
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

    let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 100);

    mqtt_client
        .subscribe("ixmati/evt/#", QoS::AtLeastOnce)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to subscribe to ixmati/evt/#");
            std::process::exit(1);
        });

    tracing::info!("subscribed to ixmati/evt/#");

    let seen: Arc<Mutex<HashSet<String>>> =
        Arc::new(Mutex::new(HashSet::with_capacity(dedup_capacity)));
    let mut processed: u64 = 0;
    let mut skipped: u64 = 0;
    let mut latest_versions: HashMap<(String, String, String), u64> = HashMap::new();

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                let event: EventEnvelope = match serde_json::from_slice(&publish.payload) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!(error = %e, topic = %publish.topic, "failed to deserialize event");
                        continue;
                    }
                };

                let version_key = (event.store.clone(), event.entity.clone(), event.key.clone());
                if latest_versions
                    .get(&version_key)
                    .is_some_and(|latest| event.version < *latest)
                {
                    skipped += 1;
                    ixmati_projector::metrics::EVENTS_SKIPPED.add(1, &[]);
                    tracing::debug!(
                        event_id = %event.event_id,
                        version = event.version,
                        "stale out-of-order event skipped"
                    );
                    continue;
                }
                latest_versions.insert(version_key, event.version);

                {
                    let mut seen_guard = seen.lock().await;
                    if seen_guard.len() >= dedup_capacity {
                        seen_guard.clear();
                        tracing::info!("dedup set cleared (reached capacity)");
                    }
                    if !seen_guard.insert(event.event_id.clone()) {
                        skipped += 1;
                        ixmati_projector::metrics::EVENTS_SKIPPED.add(1, &[]);
                        if skipped.is_multiple_of(1000) {
                            tracing::debug!(skipped, "duplicate events skipped");
                        }
                        continue;
                    }
                }

                tracing::debug!(event_id = %event.event_id, store = %event.store, entity = %event.entity, "processing event");
                engine.process_event_async(&event, &cache).await;
                ixmati_projector::metrics::record_processed(&event.occurred_at);
                ixmati_projector::metrics::MQTT_CONNECTED.record(1, &[]);
                processed += 1;

                if processed.is_multiple_of(1000) {
                    tracing::info!(
                        processed,
                        skipped,
                        seen = seen.lock().await.len(),
                        "projector status"
                    );
                }
            }
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                ixmati_projector::metrics::MQTT_CONNECTED.record(1, &[]);
                tracing::info!("MQTT connected");
            }
            Ok(Event::Incoming(other)) => {
                tracing::trace!(event = ?other, "mqtt event");
            }
            Ok(_) => {}
            Err(e) => {
                ixmati_projector::metrics::MQTT_CONNECTED.record(0, &[]);
                ixmati_projector::metrics::MQTT_EVENTLOOP_ERRORS.add(1, &[]);
                tracing::error!(error = %e, "MQTT eventloop error, reconnecting in 5s");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                let (new_client, new_eventloop) =
                    AsyncClient::new(MqttOptions::new("ixmati-projector", &host, port), 100);
                eventloop = new_eventloop;
                new_client
                    .subscribe("ixmati/evt/#", QoS::AtLeastOnce)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!(error = %e, "failed to re-subscribe");
                        std::process::exit(1);
                    });
            }
        }
    }
}

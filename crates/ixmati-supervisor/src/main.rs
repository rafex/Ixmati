use ixmati_core::{StoreRegistry, Topology};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_supervisor=info".into()),
        )
        .json()
        .init();

    let config = ixmati_core::StoreConfig {
        name: "pedidos".into(),
        label: Some("Pedidos".into()),
        db_path: "/data/ixmati/pedidos.db".into(),
        topic_cmd: String::new(),
        topic_evt: String::new(),
        mqtt_broker: std::env::var("MQTT_BROKER").unwrap_or_else(|_| "tcp://localhost:1883".into()),
        mqtt_client_id: "supervisor".into(),
        batch_size: 100,
        batch_interval_ms: 50,
        litestream_config: None,
    };

    let registry = StoreRegistry::new(vec![config], Topology::SingleProcess);
    let supervisor = ixmati_supervisor::Supervisor::new(registry);

    tracing::info!(stores = supervisor.store_count(), topology = ?supervisor.topology(), "ixmati-supervisor running");
    tokio::signal::ctrl_c().await.ok();
}

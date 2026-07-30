use ixmati_writer::Writer;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_writer=info".into()),
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
        mqtt_client_id: format!("writer-{}", uuid::Uuid::new_v4()),
        batch_size: 100,
        batch_interval_ms: 50,
        litestream_config: None,
    };

    let writer = Writer::new(&config);
    tracing::info!(store = %writer.store_name(), db = %writer.db_path(), "ixmati-writer starting");
    Ok(())
}

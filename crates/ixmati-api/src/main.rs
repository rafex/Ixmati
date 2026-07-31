use ixmati_api::ApiConfig;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_api=info".into()),
        )
        .json()
        .init();

    let db_path = std::env::args().nth(1);

    let config = ApiConfig {
        host: std::env::var("API_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: std::env::var("API_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(30000),
        mqtt_broker: std::env::var("MQTT_BROKER").unwrap_or_else(|_| "tcp://localhost:1883".into()),
        db_path,
    };

    ixmati_api::serve(config).await
}

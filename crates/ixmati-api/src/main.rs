use ixmati_api::ApiConfig;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_api=info".into()),
        )
        .json()
        .init();

    let db_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("SQLITE_PATH").ok());

    let db_paths = std::env::var("SQLITE_PATHS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .filter_map(|entry| entry.split_once('='))
                .map(|(store, path)| (store.trim().to_string(), path.trim().to_string()))
                .filter(|(store, path)| !store.is_empty() && !path.is_empty())
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    let cache_dir = std::env::var("CACHE_DIR").ok();

    let config = ApiConfig {
        host: std::env::var("API_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: std::env::var("API_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(30000),
        grpc_host: std::env::var("GRPC_HOST")
            .or_else(|_| std::env::var("API_HOST"))
            .unwrap_or_else(|_| "127.0.0.1".into()),
        grpc_port: std::env::var("GRPC_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(30100),
        mqtt_broker: std::env::var("MQTT_BROKER").unwrap_or_else(|_| "tcp://localhost:1883".into()),
        db_path,
        db_paths,
        cache_dir,
    };

    ixmati_api::serve(config).await
}

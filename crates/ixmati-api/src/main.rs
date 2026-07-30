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

    let config = ApiConfig::default();
    ixmati_api::serve(config).await
}

use ixmati_cache::NoOpBackend;
use ixmati_core::ProjectionRegistry;
use ixmati_projector::ProjectorEngine;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_projector=info".into()),
        )
        .json()
        .init();

    let registry = ProjectionRegistry::new(vec![]);
    let cache = Arc::new(NoOpBackend::new());
    let engine = ProjectorEngine::new(registry, cache);

    tracing::info!("ixmati-projector running");
    let _engine = engine;
    tokio::signal::ctrl_c().await.ok();
}

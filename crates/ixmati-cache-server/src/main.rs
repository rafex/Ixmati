use ixmati_cache::{CacheBackend, CacheServer, RedbCacheBackend, SqliteCacheBackend};
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_cache_server=info".into()),
        )
        .json()
        .init();

    let cache_dir = std::env::var("CACHE_DIR").unwrap_or_else(|_| "/var/lib/ixmati/cache".into());
    let backend = std::env::var("CACHE_BACKEND").unwrap_or_else(|_| "redb".into());
    let socket_path = std::env::var("CACHE_SOCKET_PATH")
        .unwrap_or_else(|_| "/var/run/ixmati/cache.sock".into());

    tracing::info!(dir = %cache_dir, backend = %backend, socket = %socket_path, "ixmati-cache-server starting");

    let cache: Arc<dyn CacheBackend> = match backend.as_str() {
        "redb" => {
            let path = format!("{cache_dir}/cache.redb");
            match RedbCacheBackend::new(&path) {
                Ok(b) => {
                    tracing::info!(path = %path, "RedbCacheBackend initialized");
                    Arc::new(b)
                }
                Err(e) => {
                    tracing::error!(error = %e, "RedbCacheBackend init failed");
                    return Err(std::io::Error::other(format!("Redb init: {e}")));
                }
            }
        }
        "sqlite" => {
            let path = format!("{cache_dir}/cache.db");
            match SqliteCacheBackend::new(&path) {
                Ok(b) => {
                    tracing::info!(path = %path, "SqliteCacheBackend initialized");
                    Arc::new(b)
                }
                Err(e) => {
                    tracing::error!(error = %e, "SqliteCacheBackend init failed");
                    return Err(std::io::Error::other(format!("Sqlite init: {e}")));
                }
            }
        }
        other => {
            tracing::error!(backend = %other, "unknown CACHE_BACKEND");
            return Err(std::io::Error::other(format!(
                "unknown CACHE_BACKEND: {other}"
            )));
        }
    };

    let server = CacheServer::new(cache, &socket_path);
    server.run().await
}

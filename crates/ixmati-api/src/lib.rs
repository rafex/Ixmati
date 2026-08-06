pub mod auth;
pub mod cache_client;
pub mod cache_proxy;
pub mod health;
pub mod metrics;
pub mod rest;
pub mod status;
pub mod throttle;

use axum::Router;
use ixmati_cache::CacheBackend;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub mqtt_broker: String,
    pub db_path: Option<String>,
    pub cache_dir: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8080,
            mqtt_broker: "tcp://localhost:1883".into(),
            db_path: None,
            cache_dir: None,
        }
    }
}

pub async fn serve(config: ApiConfig) -> std::io::Result<()> {
    let backend = std::env::var("CACHE_BACKEND").unwrap_or_default();
    let cache_read_mode = std::env::var("CACHE_READ_MODE").unwrap_or_default();

    let cache: Arc<dyn CacheBackend> = if let Some(ref dir) = config.cache_dir {
        if cache_read_mode != "mqtt" && cache_read_mode != "socket" {
            match backend.as_str() {
                #[cfg(feature = "flashdb")]
                "flashdb" => match ixmati_cache::FlashDb::new(dir) {
                    Ok(fdb) => {
                        tracing::info!(cache_dir = %dir, "FlashDB initialized for API (read-only)");
                        Arc::new(ixmati_cache::ReadOnlyCache::new(fdb))
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "FlashDB init failed, falling back to NoOp");
                        Arc::new(ixmati_cache::NoOpBackend::new())
                    }
                },
                "redb" => {
                    let path = format!("{}/cache.redb", dir);
                    match ixmati_cache::RedbCacheBackend::new_readonly(&path) {
                        Ok(b) => {
                            tracing::info!(cache_path = %path, "RedbCacheBackend initialized for API");
                            Arc::new(ixmati_cache::ReadOnlyCache::new(b))
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Redb init failed, falling back to NoOp");
                            Arc::new(ixmati_cache::NoOpBackend::new())
                        }
                    }
                }
                "sqlite" => {
                    let path = format!("{}/cache.db", dir);
                    match ixmati_cache::SqliteCacheBackend::new(&path) {
                        Ok(b) => {
                            tracing::info!(cache_path = %path, "SqliteCacheBackend initialized for API");
                            Arc::new(b)
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "SqliteCache init failed, falling back to NoOp");
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
        }
    } else {
        tracing::info!("no cache_dir configured, using NoOp backend");
        Arc::new(ixmati_cache::NoOpBackend::new())
    };

    let (cache_proxy, proxy_client) = if cache_read_mode == "mqtt" {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let (proxy, client) = crate::cache_proxy::CacheProxy::new(&config.mqtt_broker);
        tracing::info!("CacheProxy initialized (MQTT mode)");
        (Some(proxy), Some(client))
    } else {
        (None, None)
    };

    let cache_socket: Option<Arc<crate::cache_client::CacheClient>> = if cache_read_mode == "socket" {
        let socket_path = std::env::var("CACHE_SOCKET_PATH")
            .unwrap_or_else(|_| "/var/run/ixmati/cache.sock".into());
        let mut client = None;
        for _ in 0..30 {
            match crate::cache_client::CacheClient::connect(&socket_path).await {
                Ok(c) => { client = Some(Arc::new(c)); break; }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
            }
        }
        match client {
            Some(client) => {
                tracing::info!(path = %socket_path, "CacheClient connected (socket mode)");
                Some(client)
            }
            None => {
                tracing::warn!(path = %socket_path, "CacheClient connect failed after retries");
                None
            }
        }
    } else {
        None
    };

    let cache_mode_for_with_db = cache_proxy.clone();
    let proxy_client_for_with_db = proxy_client.clone();

    let mut state = rest::AppState::new(
        &config.mqtt_broker,
        Arc::clone(&cache),
        cache_proxy,
        proxy_client,
        cache_socket.clone(),
    );
    if let Some(ref db) = config.db_path {
        let cm = if let Some(p) = cache_mode_for_with_db {
            rest::CacheReadMode::Mqtt(p)
        } else if let Some(ref s) = cache_socket {
            rest::CacheReadMode::Socket(Arc::clone(s))
        } else {
            rest::CacheReadMode::Direct
        };
        state = rest::AppState::with_db(
            &config.mqtt_broker,
            db,
            Arc::clone(&cache),
            cache_socket,
            cm,
            Arc::new(tokio::sync::Mutex::new(proxy_client_for_with_db)),
        );
    }

    let auth_config = crate::auth::AuthConfig::new(
        std::env::var("IXMATI_API_KEYS")
            .unwrap_or_default()
            .split(',')
            .filter(|k| !k.is_empty())
            .map(|k| crate::auth::ApiKey {
                key_id: k.trim().to_string(),
                store_access: Vec::new(),
                created_at: chrono::Utc::now(),
            })
            .collect(),
    );

    let app = Router::new().merge(rest::routes(state, auth_config));

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    tracing::info!(%addr, "ixmati-api starting");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = ApiConfig::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 8080);
    }
}

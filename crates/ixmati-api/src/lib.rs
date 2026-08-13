pub mod auth;
pub mod backpressure;
pub mod cache_client;
pub mod cache_proxy;
pub mod grpc;
pub mod health;
pub mod metrics;
pub mod rest;
pub mod self_monitor;
pub mod status;
pub mod throttle;

use axum::Router;
use ixmati_cache::CacheBackend;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub grpc_host: String,
    pub grpc_port: u16,
    pub mqtt_broker: String,
    pub db_path: Option<String>,
    pub db_paths: HashMap<String, String>,
    pub cache_dir: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8080,
            grpc_host: "127.0.0.1".into(),
            grpc_port: 30100,
            mqtt_broker: "tcp://localhost:1883".into(),
            db_path: None,
            db_paths: HashMap::new(),
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

    let cache_socket: Option<Arc<crate::cache_client::CacheClient>> = if cache_read_mode == "socket"
    {
        let socket_path = std::env::var("CACHE_SOCKET_PATH")
            .unwrap_or_else(|_| "/var/run/ixmati/cache.sock".into());
        let mut client = None;
        for _ in 0..30 {
            match crate::cache_client::CacheClient::connect(&socket_path).await {
                Ok(c) => {
                    client = Some(Arc::new(c));
                    break;
                }
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
    } else if !config.db_paths.is_empty() {
        state = state.with_db_paths(config.db_paths.clone());
    }

    let api_keys = std::env::var("IXMATI_API_KEYS").unwrap_or_default();
    let scoped_keys = std::env::var("IXMATI_API_KEY_SCOPES").unwrap_or_default();
    let scopes = parse_api_key_scopes(&scoped_keys);
    let auth_config = crate::auth::AuthConfig::new(
        api_keys
            .split(',')
            .filter(|key| !key.trim().is_empty())
            .map(|key| {
                let key = key.trim();
                crate::auth::ApiKey {
                    key_id: key.to_string(),
                    store_access: scopes.get(key).cloned().unwrap_or_default(),
                    created_at: chrono::Utc::now(),
                }
            })
            .collect(),
    );

    self_monitor::spawn(
        config.db_path.clone(),
        config.db_paths.clone(),
        state.outbox_backlog(),
    );

    let app = Router::new().merge(rest::routes(state.clone(), auth_config.clone()));

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    tracing::info!(%addr, "ixmati-api starting");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let rest_server = axum::serve(listener, app);
    if config.grpc_port == 0 {
        return rest_server.await;
    }

    let grpc_host = config.grpc_host;
    let grpc_port = config.grpc_port;
    tokio::select! {
        result = rest_server => result,
        result = crate::grpc::serve(state, auth_config, &grpc_host, grpc_port) => {
            result.map_err(|error| std::io::Error::other(error.to_string()))
        }
    }
}

fn parse_api_key_scopes(raw: &str) -> HashMap<String, Vec<String>> {
    raw.split(';')
        .filter_map(|entry| entry.split_once('='))
        .map(|(key, stores)| {
            (
                key.trim().to_string(),
                stores
                    .split('|')
                    .map(str::trim)
                    .filter(|store| !store.is_empty())
                    .map(str::to_string)
                    .collect(),
            )
        })
        .filter(|(key, _)| !key.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = ApiConfig::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.grpc_port, 30100);
    }

    #[test]
    fn api_key_scopes_parse_without_changing_legacy_keys() {
        let scopes = parse_api_key_scopes("orders-key=orders|users; audit-key=audit");
        assert_eq!(scopes["orders-key"], vec!["orders", "users"]);
        assert_eq!(scopes["audit-key"], vec!["audit"]);
        assert!(parse_api_key_scopes("").is_empty());
    }
}

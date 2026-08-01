pub mod auth;
pub mod health;
pub mod metrics;
pub mod rest;
pub mod status;
pub mod throttle;

use axum::Router;
use std::net::SocketAddr;

pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub mqtt_broker: String,
    pub db_path: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8080,
            mqtt_broker: "tcp://localhost:1883".into(),
            db_path: None,
        }
    }
}

pub async fn serve(config: ApiConfig) -> std::io::Result<()> {
    let mut state = rest::AppState::new(&config.mqtt_broker);
    if let Some(ref db) = config.db_path {
        state = rest::AppState::with_db(&config.mqtt_broker, db);
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

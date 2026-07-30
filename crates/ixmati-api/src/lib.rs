pub mod rest;
pub mod status;

use axum::Router;
use std::net::SocketAddr;

pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub mqtt_broker: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8080,
            mqtt_broker: "tcp://localhost:1883".into(),
        }
    }
}

pub async fn serve(config: ApiConfig) -> std::io::Result<()> {
    let state = rest::AppState::new(&config.mqtt_broker);

    let app = Router::new()
        .merge(rest::routes(state));

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

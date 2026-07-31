use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub overall: Health,
    pub components: Vec<ComponentHealth>,
    pub store: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: Health,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Health {
    Ok,
    Degraded,
    Unavailable,
}

pub struct HealthChecker {
    db_path: Option<String>,
    mqtt_broker: Option<String>,
    store_name: Option<String>,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            db_path: None,
            mqtt_broker: None,
            store_name: None,
        }
    }

    pub fn with_db(mut self, path: &str) -> Self {
        self.db_path = Some(path.to_string());
        self
    }

    pub fn with_mqtt(mut self, broker: &str) -> Self {
        self.mqtt_broker = Some(broker.to_string());
        self
    }

    pub fn with_store(mut self, name: &str) -> Self {
        self.store_name = Some(name.to_string());
        self
    }

    pub fn check(&self) -> HealthStatus {
        let mut components = Vec::new();

        components.push(ComponentHealth {
            name: "api".into(),
            status: Health::Ok,
            detail: Some("running".into()),
        });

        if let Some(ref path) = self.db_path {
            components.push(self.check_sqlite(path));
        }

        if let Some(ref broker) = self.mqtt_broker {
            components.push(self.check_mosquitto(broker));
        }

        let overall = components
            .iter()
            .map(|c| &c.status)
            .fold(Health::Ok, |acc, s| worst(acc, s.clone()));

        HealthStatus {
            overall,
            components,
            store: self.store_name.clone(),
        }
    }

    fn check_sqlite(&self, path: &str) -> ComponentHealth {
        match rusqlite::Connection::open(path) {
            Ok(conn) => match conn.query_row("SELECT 1", [], |_| Ok(())) {
                Ok(()) => ComponentHealth {
                    name: "sqlite".into(),
                    status: Health::Ok,
                    detail: Some(format!("connected to {}", path)),
                },
                Err(e) => ComponentHealth {
                    name: "sqlite".into(),
                    status: Health::Degraded,
                    detail: Some(e.to_string()),
                },
            },
            Err(e) => ComponentHealth {
                name: "sqlite".into(),
                status: Health::Unavailable,
                detail: Some(e.to_string()),
            },
        }
    }

    fn check_mosquitto(&self, broker: &str) -> ComponentHealth {
        use std::net::TcpStream;

        let addr = broker
            .trim_start_matches("tcp://")
            .trim_start_matches("mqtt://");

        match TcpStream::connect_timeout(
            &addr
                .parse()
                .unwrap_or_else(|_| "127.0.0.1:1883".parse().unwrap()),
            Duration::from_secs(2),
        ) {
            Ok(_) => ComponentHealth {
                name: "mosquitto".into(),
                status: Health::Ok,
                detail: Some(format!("{} reachable", broker)),
            },
            Err(e) => ComponentHealth {
                name: "mosquitto".into(),
                status: Health::Unavailable,
                detail: Some(e.to_string()),
            },
        }
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

fn worst(a: Health, b: Health) -> Health {
    match (a, b) {
        (Health::Unavailable, _) | (_, Health::Unavailable) => Health::Unavailable,
        (Health::Degraded, _) | (_, Health::Degraded) => Health::Degraded,
        _ => Health::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_check_api_only_returns_ok() {
        let checker = HealthChecker::new();
        let status = checker.check();

        assert_eq!(status.overall, Health::Ok);
        assert!(!status.components.is_empty());
    }

    #[test]
    fn health_check_with_db_and_mqtt() {
        let db_path = format!("{}/ixmati-health-test.db", std::env::temp_dir().display());

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS _health (id INTEGER PRIMARY KEY);")
            .unwrap();
        drop(conn);

        let checker = HealthChecker::new()
            .with_db(&db_path)
            .with_mqtt("tcp://localhost:1883");

        let status = checker.check();
        assert!(status.components.len() >= 2);

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn worst_health_aggregation() {
        assert_eq!(worst(Health::Ok, Health::Degraded), Health::Degraded);
        assert_eq!(worst(Health::Unavailable, Health::Ok), Health::Unavailable);
        assert_eq!(worst(Health::Ok, Health::Ok), Health::Ok);
    }

    #[test]
    fn health_check_with_store_name() {
        let checker = HealthChecker::new().with_store("pedidos");
        let status = checker.check();
        assert_eq!(status.store, Some("pedidos".into()));
    }
}

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
    store_db_paths: Vec<(String, String)>,
    mqtt_broker: Option<String>,
    store_name: Option<String>,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            db_path: None,
            store_db_paths: Vec::new(),
            mqtt_broker: None,
            store_name: None,
        }
    }

    pub fn with_db(mut self, path: &str) -> Self {
        self.db_path = Some(path.to_string());
        self
    }

    pub fn with_store_db(mut self, store: &str, path: &str) -> Self {
        self.store_db_paths
            .push((store.to_string(), path.to_string()));
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
            components.push(self.check_sqlite("sqlite", path));
        }

        for (store, path) in &self.store_db_paths {
            components.push(self.check_sqlite(&format!("sqlite:{store}"), path));
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

    fn check_sqlite(&self, name: &str, path: &str) -> ComponentHealth {
        match rusqlite::Connection::open(path) {
            Ok(conn) => match conn.query_row("SELECT 1", [], |_| Ok(())) {
                Ok(()) => ComponentHealth {
                    name: name.into(),
                    status: Health::Ok,
                    detail: Some(format!("connected to {}", path)),
                },
                Err(e) => ComponentHealth {
                    name: name.into(),
                    status: Health::Degraded,
                    detail: Some(e.to_string()),
                },
            },
            Err(e) => ComponentHealth {
                name: name.into(),
                status: Health::Unavailable,
                detail: Some(e.to_string()),
            },
        }
    }

    fn check_mosquitto(&self, broker: &str) -> ComponentHealth {
        use std::net::{TcpStream, ToSocketAddrs};

        let addr = broker
            .trim_start_matches("tcp://")
            .trim_start_matches("mqtt://");

        let reachable = addr
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| {
                addrs.find_map(|socket| {
                    TcpStream::connect_timeout(&socket, Duration::from_secs(2)).ok()
                })
            })
            .is_some();

        match reachable {
            true => ComponentHealth {
                name: "mosquitto".into(),
                status: Health::Ok,
                detail: Some(format!("{} reachable", broker)),
            },
            false => {
                let detail = format!("unable to connect to {}", broker);
                ComponentHealth {
                    name: "mosquitto".into(),
                    status: Health::Unavailable,
                    detail: Some(detail),
                }
            }
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

    #[test]
    fn health_check_includes_each_configured_store() {
        let first = format!(
            "{}/ixmati-health-first-{}.db",
            std::env::temp_dir().display(),
            uuid::Uuid::new_v4()
        );
        let second = format!(
            "{}/ixmati-health-second-{}.db",
            std::env::temp_dir().display(),
            uuid::Uuid::new_v4()
        );
        rusqlite::Connection::open(&first).unwrap();
        rusqlite::Connection::open(&second).unwrap();

        let status = HealthChecker::new()
            .with_store_db("pedidos", &first)
            .with_store_db("usuarios", &second)
            .check();

        let names: Vec<_> = status
            .components
            .iter()
            .map(|component| component.name.as_str())
            .collect();
        assert!(names.contains(&"sqlite:pedidos"));
        assert!(names.contains(&"sqlite:usuarios"));

        let _ = std::fs::remove_file(first);
        let _ = std::fs::remove_file(second);
    }
}

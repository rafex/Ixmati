use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub stores: Vec<StoreConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreConfig {
    pub name: String,
    pub label: Option<String>,
    pub db_path: String,
    pub topic_cmd: String,
    pub topic_evt: String,
    pub mqtt_broker: String,
    pub mqtt_client_id: String,
    pub batch_size: usize,
    pub batch_interval_ms: u64,
    pub litestream_config: Option<String>,
}

impl Config {
    pub fn validate(&self) -> std::result::Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.stores.is_empty() {
            errors.push("at least one store required".into());
        }

        let names: std::collections::HashSet<&str> =
            self.stores.iter().map(|s| s.name.as_str()).collect();

        if names.len() != self.stores.len() {
            errors.push("duplicate store names".into());
        }

        for store in &self.stores {
            if store.name.is_empty() {
                errors.push("store name must not be empty".into());
            }
            if store.db_path.is_empty() {
                errors.push(format!("store '{}': db_path must not be empty", store.name));
            }
            if store.batch_size == 0 {
                errors.push(format!(
                    "store '{}': batch_size must be positive",
                    store.name
                ));
            }
            store.validate_name()?;
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn is_single_store(&self) -> bool {
        self.stores.len() == 1
    }
}

impl StoreConfig {
    pub fn topic_cmd(&self) -> String {
        if self.topic_cmd.is_empty() {
            format!("ixmati/cmd/{}", self.name)
        } else {
            self.topic_cmd.clone()
        }
    }

    pub fn topic_evt(&self) -> String {
        if self.topic_evt.is_empty() {
            format!("ixmati/evt/{}", self.name)
        } else {
            self.topic_evt.clone()
        }
    }

    fn validate_name(&self) -> std::result::Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.name.contains('/') {
            errors.push(format!(
                "store '{}': name must not contain '/'",
                self.name
            ));
        }
        if self.name.contains(char::is_whitespace) {
            errors.push(format!(
                "store '{}': name must not contain whitespace",
                self.name
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_single_store_config() {
        let cfg = Config {
            stores: vec![StoreConfig {
                name: "pedidos".into(),
                label: Some("Pedidos".into()),
                db_path: "/data/pedidos.db".into(),
                topic_cmd: String::new(),
                topic_evt: String::new(),
                mqtt_broker: "tcp://localhost:1883".into(),
                mqtt_client_id: "writer-pedidos".into(),
                batch_size: 100,
                batch_interval_ms: 50,
                litestream_config: None,
            }],
        };

        assert!(cfg.validate().is_ok());
        assert!(cfg.is_single_store());
    }

    #[test]
    fn empty_stores_invalid() {
        let cfg = Config { stores: vec![] };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn store_name_with_slash_rejected() {
        let cfg = Config {
            stores: vec![StoreConfig {
                name: "bad/name".into(),
                label: None,
                db_path: "/data/bad.db".into(),
                topic_cmd: String::new(),
                topic_evt: String::new(),
                mqtt_broker: "tcp://localhost:1883".into(),
                mqtt_client_id: "writer-bad".into(),
                batch_size: 1,
                batch_interval_ms: 50,
                litestream_config: None,
            }],
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn topic_defaults() {
        let store = StoreConfig {
            name: "pedidos".into(),
            label: None,
            db_path: "/data/pedidos.db".into(),
            topic_cmd: String::new(),
            topic_evt: String::new(),
            mqtt_broker: "tcp://localhost:1883".into(),
            mqtt_client_id: "writer-pedidos".into(),
            batch_size: 100,
            batch_interval_ms: 50,
            litestream_config: None,
        };

        assert_eq!(store.topic_cmd(), "ixmati/cmd/pedidos");
        assert_eq!(store.topic_evt(), "ixmati/evt/pedidos");
    }
}

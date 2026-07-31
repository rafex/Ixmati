use ixmati_core::{StoreRegistry, Topology};
use std::collections::HashMap;

pub struct Supervisor {
    registry: StoreRegistry,
    writers: HashMap<String, WriterHandle>,
}

pub struct WriterHandle {
    pub store_name: String,
    pub db_path: String,
    pub topic_cmd: String,
    pub topic_evt: String,
    pub status: WriterStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WriterStatus {
    Initialized,
    Running,
    Stopped,
    Failed(String),
}

impl Supervisor {
    pub fn new(registry: StoreRegistry) -> Self {
        let mut writers = HashMap::new();

        for store in registry.stores() {
            writers.insert(
                store.name().to_string(),
                WriterHandle {
                    store_name: store.name().to_string(),
                    db_path: store.db_path().to_string(),
                    topic_cmd: store.config.topic_cmd(),
                    topic_evt: store.config.topic_evt(),
                    status: WriterStatus::Initialized,
                },
            );
        }

        Self { registry, writers }
    }

    pub fn store_count(&self) -> usize {
        self.writers.len()
    }

    pub fn topology(&self) -> Topology {
        self.registry.topology()
    }

    pub fn writers(&self) -> &HashMap<String, WriterHandle> {
        &self.writers
    }

    pub fn writer_for(&self, store: &str) -> Option<&WriterHandle> {
        self.writers.get(store)
    }

    pub fn mark_running(&mut self, store: &str) {
        if let Some(handle) = self.writers.get_mut(store) {
            handle.status = WriterStatus::Running;
        }
    }

    pub fn mark_stopped(&mut self, store: &str) {
        if let Some(handle) = self.writers.get_mut(store) {
            handle.status = WriterStatus::Stopped;
        }
    }

    pub fn mark_failed(&mut self, store: &str, error: String) {
        if let Some(handle) = self.writers.get_mut(store) {
            handle.status = WriterStatus::Failed(error);
        }
    }

    pub fn should_activate_events(&self) -> bool {
        self.registry.should_activate_events()
    }

    pub fn topics_cmd(&self) -> Vec<String> {
        self.registry.topics_cmd()
    }

    pub fn topics_evt(&self) -> Vec<String> {
        self.registry.topics_evt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixmati_core::StoreConfig;

    fn make_store(name: &str) -> StoreConfig {
        StoreConfig {
            name: name.into(),
            label: None,
            db_path: format!("/data/{}.db", name),
            topic_cmd: String::new(),
            topic_evt: String::new(),
            mqtt_broker: "tcp://localhost:1883".into(),
            mqtt_client_id: format!("writer-{}", name),
            batch_size: 100,
            batch_interval_ms: 50,
            litestream_config: None,
        }
    }

    #[test]
    fn supervisor_creates_writers_for_each_store() {
        let registry = StoreRegistry::new(
            vec![make_store("pedidos"), make_store("usuarios")],
            Topology::SingleProcess,
        );

        let supervisor = Supervisor::new(registry);
        assert_eq!(supervisor.store_count(), 2);
        assert!(supervisor.writer_for("pedidos").is_some());
        assert!(supervisor.writer_for("usuarios").is_some());
    }

    #[test]
    fn single_store_no_events() {
        let registry = StoreRegistry::new(vec![make_store("pedidos")], Topology::PodPerStore);

        let supervisor = Supervisor::new(registry);
        assert_eq!(supervisor.store_count(), 1);
        assert!(!supervisor.should_activate_events());
    }

    #[test]
    fn multi_store_activates_events() {
        let registry = StoreRegistry::new(
            vec![
                make_store("pedidos"),
                make_store("usuarios"),
                make_store("catalogo"),
            ],
            Topology::PodPerStore,
        );

        let supervisor = Supervisor::new(registry);
        assert!(supervisor.should_activate_events());
    }

    #[test]
    fn writer_status_transitions() {
        let registry = StoreRegistry::new(vec![make_store("pedidos")], Topology::SingleProcess);

        let mut supervisor = Supervisor::new(registry);
        assert_eq!(
            supervisor.writer_for("pedidos").unwrap().status,
            WriterStatus::Initialized
        );

        supervisor.mark_running("pedidos");
        assert_eq!(
            supervisor.writer_for("pedidos").unwrap().status,
            WriterStatus::Running
        );

        supervisor.mark_stopped("pedidos");
        assert_eq!(
            supervisor.writer_for("pedidos").unwrap().status,
            WriterStatus::Stopped
        );

        supervisor.mark_failed("pedidos", "disk full".into());
        match &supervisor.writer_for("pedidos").unwrap().status {
            WriterStatus::Failed(msg) => assert_eq!(msg, "disk full"),
            _ => panic!("expected Failed status"),
        }
    }

    #[test]
    fn topics_generated_for_all_stores() {
        let registry = StoreRegistry::new(
            vec![make_store("pedidos"), make_store("usuarios")],
            Topology::PodPerStore,
        );

        let supervisor = Supervisor::new(registry);
        assert_eq!(
            supervisor.topics_cmd(),
            vec!["ixmati/cmd/pedidos", "ixmati/cmd/usuarios"]
        );
        assert_eq!(
            supervisor.topics_evt(),
            vec!["ixmati/evt/pedidos", "ixmati/evt/usuarios"]
        );
    }
}

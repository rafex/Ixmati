use crate::config::StoreConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topology {
    SingleProcess,
    PodPerStore,
}

#[derive(Debug)]
pub struct StoreRegistry {
    stores: Vec<Store>,
    topology: Topology,
}

#[derive(Debug)]
pub struct Store {
    pub config: StoreConfig,
}

impl StoreRegistry {
    pub fn new(configs: Vec<StoreConfig>, topology: Topology) -> Self {
        let stores = configs.into_iter().map(Store::new).collect();
        Self { stores, topology }
    }

    pub fn len(&self) -> usize {
        self.stores.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stores.is_empty()
    }

    pub fn is_single_store(&self) -> bool {
        self.stores.len() == 1
    }

    pub fn topology(&self) -> Topology {
        self.topology
    }

    pub fn stores(&self) -> &[Store] {
        &self.stores
    }

    pub fn get(&self, name: &str) -> Option<&Store> {
        self.stores.iter().find(|s| s.config.name == name)
    }

    pub fn owns(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn should_activate_events(&self) -> bool {
        self.stores.len() > 1
    }

    pub fn should_activate_outbox(&self) -> bool {
        self.should_activate_events()
    }

    pub fn should_activate_projectors(&self) -> bool {
        self.should_activate_events()
    }

    pub fn writer_for(&self, store_name: &str) -> Option<&Store> {
        self.get(store_name)
    }

    pub fn topics_cmd(&self) -> Vec<String> {
        self.stores.iter().map(|s| s.config.topic_cmd()).collect()
    }

    pub fn topics_evt(&self) -> Vec<String> {
        self.stores.iter().map(|s| s.config.topic_evt()).collect()
    }
}

impl Store {
    pub fn new(config: StoreConfig) -> Self {
        Self { config }
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn db_path(&self) -> &str {
        &self.config.db_path
    }

    pub fn entity_topic(&self, entity: &str, id: &str) -> String {
        format!("ixmati/cmd/{}/{}/{}", self.config.name, entity, id)
    }

    pub fn event_topic(&self, entity: &str, id: &str) -> String {
        format!("ixmati/evt/{}/{}/{}", self.config.name, entity, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoreConfig;

    fn make_config(name: &str) -> StoreConfig {
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
    fn single_store_registry_no_events() {
        let reg = StoreRegistry::new(vec![make_config("pedidos")], Topology::SingleProcess);

        assert!(reg.is_single_store());
        assert_eq!(reg.len(), 1);
        assert!(!reg.should_activate_events());
        assert!(!reg.should_activate_outbox());
        assert!(!reg.should_activate_projectors());
    }

    #[test]
    fn multi_store_registry_activates_events() {
        let reg = StoreRegistry::new(
            vec![make_config("pedidos"), make_config("usuarios"), make_config("catalogo")],
            Topology::PodPerStore,
        );

        assert!(!reg.is_single_store());
        assert_eq!(reg.len(), 3);
        assert!(reg.should_activate_events());
        assert!(reg.should_activate_outbox());
        assert!(reg.should_activate_projectors());
    }

    #[test]
    fn store_lookup_by_name() {
        let reg = StoreRegistry::new(
            vec![make_config("pedidos"), make_config("usuarios")],
            Topology::SingleProcess,
        );

        assert!(reg.owns("pedidos"));
        assert!(reg.owns("usuarios"));
        assert!(!reg.owns("inexistente"));

        let store = reg.get("pedidos").unwrap();
        assert_eq!(store.name(), "pedidos");
    }

    #[test]
    fn topics_generation() {
        let reg = StoreRegistry::new(
            vec![make_config("pedidos")],
            Topology::SingleProcess,
        );

        assert_eq!(reg.topics_cmd(), vec!["ixmati/cmd/pedidos"]);
        assert_eq!(reg.topics_evt(), vec!["ixmati/evt/pedidos"]);
    }

    #[test]
    fn entity_topic_format() {
        let store = Store::new(make_config("pedidos"));
        assert_eq!(
            store.entity_topic("pedido", "ped_abc123"),
            "ixmati/cmd/pedidos/pedido/ped_abc123"
        );
        assert_eq!(
            store.event_topic("pedido", "ped_abc123"),
            "ixmati/evt/pedidos/pedido/ped_abc123"
        );
    }

    #[test]
    fn topology_is_stored() {
        let reg_sp = StoreRegistry::new(
            vec![make_config("pedidos")],
            Topology::SingleProcess,
        );
        let reg_pp = StoreRegistry::new(
            vec![make_config("pedidos")],
            Topology::PodPerStore,
        );

        assert_eq!(reg_sp.topology(), Topology::SingleProcess);
        assert_eq!(reg_pp.topology(), Topology::PodPerStore);
    }
}

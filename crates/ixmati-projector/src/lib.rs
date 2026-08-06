pub mod pattern_m;
pub mod pattern_r;

use ixmati_cache::CacheBackend;
use ixmati_cache::CacheClient;
use ixmati_core::{EventEnvelope, ProjectionRegistry};
use std::sync::Arc;

pub struct ProjectorEngine<B: CacheBackend> {
    registry: ProjectionRegistry,
    cache: Arc<B>,
}

impl<B: CacheBackend> ProjectorEngine<B> {
    pub fn new(registry: ProjectionRegistry, cache: Arc<B>) -> Self {
        Self { registry, cache }
    }

    pub fn process_event(&self, event: &EventEnvelope) {
        let projections = self.registry.for_store(&event.store);

        for proj in projections {
            match proj.pattern {
                ixmati_core::ProjectionPattern::R => {
                    pattern_r::process_r(&*self.cache, proj, event);
                }
                ixmati_core::ProjectionPattern::M => {
                    pattern_m::process_m(&*self.cache, proj, event);
                }
            }
        }
    }

    pub fn process_events(&self, events: &[EventEnvelope]) {
        for event in events {
            self.process_event(event);
        }
    }

    pub async fn process_event_async(&self, event: &EventEnvelope, cache: &CacheClient) {
        let projections = self.registry.for_store(&event.store);

        for proj in projections {
            match proj.pattern {
                ixmati_core::ProjectionPattern::R => {
                    pattern_r::process_r_async(cache, proj, event).await;
                }
                ixmati_core::ProjectionPattern::M => {
                    pattern_m::process_m_async(cache, proj, event).await;
                }
            }
        }
    }

    pub fn registry(&self) -> &ProjectionRegistry {
        &self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixmati_cache::NoOpBackend;
    use ixmati_core::{ProjectionConfig, ProjectionPattern};
    use serde_json::json;

    fn make_event() -> EventEnvelope {
        EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "pedido.creado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_1".into(),
            version: 1,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: json!({"total": 100.0}),
        }
    }

    fn make_r_projection() -> ProjectionConfig {
        ProjectionConfig {
            name: "pedidos_con_usuario".into(),
            pattern: ProjectionPattern::R,
            source_stores: vec!["pedidos".into(), "usuarios".into()],
            target_key: "pedido_id".into(),
            ttl_seconds: 300,
            copy_fields: None,
        }
    }

    #[test]
    fn engine_processes_event_without_crash() {
        let registry = ProjectionRegistry::new(vec![make_r_projection()]);
        let cache = Arc::new(NoOpBackend::new());
        let engine = ProjectorEngine::new(registry, cache);

        let event = make_event();
        engine.process_event(&event);
    }

    #[test]
    fn engine_processes_batch() {
        let registry = ProjectionRegistry::new(vec![make_r_projection()]);
        let cache = Arc::new(NoOpBackend::new());
        let engine = ProjectorEngine::new(registry, cache);

        let events: Vec<_> = (1..=3)
            .map(|i| EventEnvelope {
                event_id: format!("evt-{}", i),
                event_type: "pedido.creado".into(),
                store: "pedidos".into(),
                entity: "pedido".into(),
                key: format!("ped_{}", i),
                version: 1,
                occurred_at: "2026-07-30T00:00:00Z".into(),
                outbox_seq: i,
                payload: json!({"total": 100 * i}),
            })
            .collect();

        engine.process_events(&events);
    }

    #[test]
    fn engine_exposes_registry() {
        let registry = ProjectionRegistry::new(vec![]);
        let cache = Arc::new(NoOpBackend::new());
        let engine = ProjectorEngine::new(registry, cache);

        assert_eq!(engine.registry().len(), 0);
    }
}

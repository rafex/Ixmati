// Pattern R: reference + lookup (eager loading)
// Example: pedidos_con_usuario -> lee pedido + usuario, une por pedido_id = usuario_id
use ixmati_cache::CacheBackend;
use ixmati_core::{EventEnvelope, ProjectionConfig};

pub fn process_r<B: CacheBackend>(cache: &B, projection: &ProjectionConfig, event: &EventEnvelope) {
    // Pattern R reads from all source stores and builds a merged projection.
    // For each source store, fetch the entity referenced by target_key.
    let mut projection_payload = serde_json::Map::new();

    for source_store in &projection.source_stores {
        let cached = cache.get(source_store, &event.entity, &event.key);

        if let Some(data) = cached {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) {
                projection_payload.insert(source_store.clone(), value);
            }
        } else if source_store == &event.store {
            projection_payload.insert(source_store.clone(), event.payload.clone());
        }
    }

    let projection_key = event_key_from_payload(event, &projection.target_key);
    let merged = serde_json::Value::Object(projection_payload);
    let merged_bytes = serde_json::to_vec(&merged).unwrap_or_default();

    // Store in cache with projection namespace
    cache.set(
        &format!("prj:{}", projection.name),
        &event.entity,
        &projection_key,
        &merged_bytes,
    );
}

fn event_key_from_payload(event: &EventEnvelope, target_key: &str) -> String {
    // Try to extract target_key from event payload
    if let Some(val) = event.payload.get(target_key) {
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
        return val.to_string();
    }
    // Fallback: use the event key
    event.key.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixmati_cache::NoOpBackend;
    use serde_json::json;

    #[test]
    fn pattern_r_does_not_crash_with_noop() {
        let cache = NoOpBackend::new();
        let proj = ProjectionConfig {
            name: "test_prj".into(),
            pattern: ixmati_core::ProjectionPattern::R,
            source_stores: vec!["pedidos".into()],
            target_key: "pedido_id".into(),
            ttl_seconds: 300,
            copy_fields: None,
        };

        let event = EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "pedido.creado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_1".into(),
            version: 1,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: json!({"pedido_id": "ped_1", "total": 100}),
        };

        process_r(&cache, &proj, &event);
    }

    #[test]
    fn extracts_target_key_from_payload() {
        let event = EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "pedido.creado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_1".into(),
            version: 1,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: json!({"pedido_id": "abc123", "total": 100}),
        };

        let key = super::event_key_from_payload(&event, "pedido_id");
        assert_eq!(key, "abc123");
    }

    #[test]
    fn falls_back_to_event_key() {
        let event = EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "pedido.creado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_1".into(),
            version: 1,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: json!({"total": 100}),
        };

        let key = super::event_key_from_payload(&event, "pedido_id");
        assert_eq!(key, "ped_1");
    }
}

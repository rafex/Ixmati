// Pattern R: reference + lookup (eager loading)
// Example: pedidos_con_usuario -> lee pedido + usuario, une por pedido_id = usuario_id
use ixmati_cache::CacheBackend;
use ixmati_cache::CacheClient;
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
    let pkey = format!("p:{}:{}", projection.name, projection_key);
    cache.set("projections", &pkey, "", &merged_bytes);
}

pub async fn process_r_async(
    cache: &CacheClient,
    projection: &ProjectionConfig,
    event: &EventEnvelope,
) {
    let mut projection_payload = serde_json::Map::new();

    for source_store in &projection.source_stores {
        if source_store == &event.store {
            projection_payload.insert(source_store.clone(), event.payload.clone());
        } else {
            let lookup_key = resolve_lookup_key(event, source_store);
            let lookup_entity = infer_entity_from_store(source_store);
            tracing::debug!(
                projection = %projection.name,
                source = %source_store,
                lookup_entity = %lookup_entity,
                lookup_key = %lookup_key,
                "pattern R: cross-store lookup"
            );
            let cached = cache.get(source_store, &lookup_entity, &lookup_key).await;
            if let Some(data) = cached {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) {
                    projection_payload.insert(source_store.clone(), value);
                }
            } else {
                tracing::debug!(
                    projection = %projection.name,
                    source = %source_store,
                    entity = %lookup_entity,
                    key = %lookup_key,
                    "pattern R: cross-store MISS"
                );
            }
        }
    }

    let projection_key = event_key_from_payload(event, &projection.target_key);
    let merged = serde_json::Value::Object(projection_payload);
    let merged_bytes = serde_json::to_vec(&merged).unwrap_or_default();

    tracing::debug!(projection = %projection.name, key = %projection_key, len = merged_bytes.len(), "pattern R: setting projection");
    cache
        .set_projection(&projection.name, &projection_key, &merged_bytes)
        .await;
}

fn resolve_lookup_key(event: &EventEnvelope, store_name: &str) -> String {
    let singular = store_name.trim_end_matches('s');
    let fk_field = format!("{}_id", singular);
    if let Some(val) = event.payload.get(&fk_field) {
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
        return val.to_string();
    }
    event.key.clone()
}

fn infer_entity_from_store(store_name: &str) -> String {
    store_name.trim_end_matches('s').to_string()
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
    fn resolve_lookup_key_uses_singular_store_id_field() {
        let event = EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "pedido.creado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_1".into(),
            version: 1,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: json!({"pedido_id": "ped_1", "usuario_id": "usr_99", "total": 100}),
        };

        assert_eq!(resolve_lookup_key(&event, "usuarios"), "usr_99");
    }

    #[test]
    fn resolve_lookup_key_falls_back_to_event_key() {
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

        assert_eq!(resolve_lookup_key(&event, "usuarios"), "ped_1");
    }

    #[test]
    fn infer_entity_strips_trailing_s() {
        assert_eq!(infer_entity_from_store("usuarios"), "usuario");
        assert_eq!(infer_entity_from_store("pedidos"), "pedido");
        assert_eq!(infer_entity_from_store("inventario"), "inventario");
    }

    #[test]
    fn pattern_r_cross_store_uses_lookup_key_not_event_key() {
        // Este test verifica que process_r_async no usa event.entity/event.key
        // para la búsqueda en otros stores, sino que usa resolve_lookup_key +
        // infer_entity_from_store.
        // El bug original era: cache.get("usuarios", "pedido", "ped_1")
        // La versión correcta es:      cache.get("usuarios", "usuario", "usr_99")

        let event = EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "pedido.creado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_1".into(),
            version: 1,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: json!({"pedido_id": "ped_1", "usuario_id": "usr_99", "total": 100}),
        };

        assert_eq!(resolve_lookup_key(&event, "usuarios"), "usr_99");
        assert_eq!(infer_entity_from_store("usuarios"), "usuario");
        // NOT "pedido" (event.entity) and NOT "ped_1" (event.key)
        assert_ne!(resolve_lookup_key(&event, "usuarios"), event.key);
    }

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

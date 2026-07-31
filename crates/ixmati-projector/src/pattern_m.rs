// Pattern M: materialized fields (copies fields from source store entities)
// Example: usuarios_materializados — copia nombre, email desde usuarios
use ixmati_cache::CacheBackend;
use ixmati_core::{EventEnvelope, ProjectionConfig};

pub fn process_m<B: CacheBackend>(cache: &B, projection: &ProjectionConfig, event: &EventEnvelope) {
    // Pattern M copies specified fields from the event payload into the projection.
    // The projection is stored in cache keyed by the target_key from the event.
    let copy = match &projection.copy_fields {
        Some(fields) => fields,
        None => return,
    };

    let mut projection_payload = serde_json::Map::new();
    let target_key = extract_target_key(event, &projection.target_key);

    // Copy only the specified fields from each source store
    for field_def in copy {
        if field_def.source_store == event.store || field_def.source_entity == event.entity {
            for field_name in &field_def.fields {
                if let Some(value) = event.payload.get(field_name) {
                    projection_payload.insert(field_name.clone(), value.clone());
                }
            }
        }
    }

    if projection_payload.is_empty() {
        return;
    }

    projection_payload.insert(
        projection.target_key.clone(),
        serde_json::Value::String(target_key.clone()),
    );

    let merged = serde_json::Value::Object(projection_payload);
    let merged_bytes = serde_json::to_vec(&merged).unwrap_or_default();

    cache.set(
        &format!("prj:{}", projection.name),
        &event.entity,
        &target_key,
        &merged_bytes,
    );
}

fn extract_target_key(event: &EventEnvelope, target_key: &str) -> String {
    if let Some(val) = event.payload.get(target_key) {
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
        return val.to_string();
    }
    event.key.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixmati_cache::NoOpBackend;
    use ixmati_core::CopyField;
    use serde_json::json;

    #[test]
    fn pattern_m_copies_specified_fields() {
        let cache = NoOpBackend::new();
        let proj = ProjectionConfig {
            name: "usuarios_materializados".into(),
            pattern: ixmati_core::ProjectionPattern::M,
            source_stores: vec!["usuarios".into()],
            target_key: "usuario_id".into(),
            ttl_seconds: 600,
            copy_fields: Some(vec![CopyField {
                source_store: "usuarios".into(),
                source_entity: "usuario".into(),
                fields: vec!["nombre".into(), "email".into()],
            }]),
        };

        let event = EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "usuario.creado".into(),
            store: "usuarios".into(),
            entity: "usuario".into(),
            key: "usr_1".into(),
            version: 1,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: json!({
                "usuario_id": "usr_1",
                "nombre": "Ana",
                "email": "ana@example.com",
                "password_hash": "secret"
            }),
        };

        process_m(&cache, &proj, &event);
    }

    #[test]
    fn pattern_m_skips_event_with_no_matching_store() {
        let cache = NoOpBackend::new();
        let proj = ProjectionConfig {
            name: "test".into(),
            pattern: ixmati_core::ProjectionPattern::M,
            source_stores: vec!["usuarios".into()],
            target_key: "usuario_id".into(),
            ttl_seconds: 600,
            copy_fields: Some(vec![CopyField {
                source_store: "usuarios".into(),
                source_entity: "usuario".into(),
                fields: vec!["nombre".into()],
            }]),
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
            payload: json!({"total": 100}),
        };

        process_m(&cache, &proj, &event);
    }
}

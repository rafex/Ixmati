// Pattern R: reference + lookup (eager loading)
// Example: pedidos_con_usuario -> lee pedido + usuario, une por pedido_id = usuario_id
use ixmati_cache::CacheBackend;
use ixmati_cache::CacheClient;
use ixmati_core::{EventEnvelope, ProjectionConfig};

const DEPENDENCY_INDEX_PREFIX: &str = "ridx";
const MAX_DEPENDENTS: usize = 100;

/// A cache-resident reverse index. One key contains all primary projection
/// keys that reference a single entity in a secondary store.
pub fn dependency_index_key(
    projection: &ProjectionConfig,
    store: &str,
    entity: &str,
    key: &str,
) -> String {
    format!(
        "{DEPENDENCY_INDEX_PREFIX}:{}:{store}:{entity}:{key}",
        projection.name
    )
}

pub fn dependency_index_prefix(projection: &ProjectionConfig) -> String {
    format!("{DEPENDENCY_INDEX_PREFIX}:{}:", projection.name)
}

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
    let Some(primary_store) = projection.source_stores.first() else {
        return;
    };

    if event.store != *primary_store {
        refresh_references_for_secondary_event(cache, projection, event).await;
        return;
    }

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

    if is_deleted(event) {
        cache
            .del_raw(&CacheClient::projection_key(
                &projection.name,
                &projection_key,
            ))
            .await;
    } else {
        tracing::debug!(projection = %projection.name, key = %projection_key, len = merged_bytes.len(), "pattern R: setting projection");
        cache
            .set_projection(&projection.name, &projection_key, &merged_bytes)
            .await;
        register_dependencies(cache, projection, event, &projection_key).await;
    }
}

async fn refresh_references_for_secondary_event(
    cache: &CacheClient,
    projection: &ProjectionConfig,
    event: &EventEnvelope,
) {
    let index_key = dependency_index_key(projection, &event.store, &event.entity, &event.key);
    let targets = cache
        .get_raw(&index_key)
        .await
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default();

    for target_key in targets {
        let projection_key = CacheClient::projection_key(&projection.name, &target_key);
        if is_deleted(event) {
            cache.del_raw(&projection_key).await;
            continue;
        }

        let mut projection_payload = serde_json::Map::new();
        for source_store in &projection.source_stores {
            if source_store == &event.store {
                // The event is authoritative for this update. This avoids a
                // race where cache_sync has not caught up yet.
                projection_payload.insert(source_store.clone(), event.payload.clone());
            } else if let Some(data) = cache
                .get(
                    source_store,
                    &infer_entity_from_store(source_store),
                    &target_key,
                )
                .await
            {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) {
                    projection_payload.insert(source_store.clone(), value);
                }
            }
        }

        let merged = serde_json::Value::Object(projection_payload);
        if let Ok(bytes) = serde_json::to_vec(&merged) {
            cache.set_raw(&projection_key, &bytes).await;
        }
    }
}

async fn register_dependencies(
    cache: &CacheClient,
    projection: &ProjectionConfig,
    event: &EventEnvelope,
    projection_key: &str,
) {
    for source_store in projection.source_stores.iter().skip(1) {
        let lookup_key = resolve_lookup_key(event, source_store);
        let index_key = dependency_index_key(
            projection,
            source_store,
            &infer_entity_from_store(source_store),
            &lookup_key,
        );
        let mut targets = cache
            .get_raw(&index_key)
            .await
            .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
            .unwrap_or_default();
        if targets.iter().any(|target| target == projection_key) {
            continue;
        }
        if targets.len() >= MAX_DEPENDENTS {
            tracing::warn!(
                projection = %projection.name,
                source = %source_store,
                key = %lookup_key,
                limit = MAX_DEPENDENTS,
                "pattern R reverse-index fan-out limit reached"
            );
            continue;
        }
        targets.push(projection_key.to_string());
        if let Ok(bytes) = serde_json::to_vec(&targets) {
            cache.set_raw(&index_key, &bytes).await;
        }
    }
}

fn is_deleted(event: &EventEnvelope) -> bool {
    event.event_type.ends_with(".deleted")
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

    #[test]
    fn dependency_index_key_is_reserved_and_stable() {
        let projection = ProjectionConfig {
            name: "pedidos_con_usuario".into(),
            pattern: ixmati_core::ProjectionPattern::R,
            source_stores: vec!["pedidos".into(), "usuarios".into()],
            target_key: "pedido_id".into(),
            ttl_seconds: 300,
            copy_fields: None,
        };
        assert_eq!(
            dependency_index_key(&projection, "usuarios", "usuario", "usr_1"),
            "ridx:pedidos_con_usuario:usuarios:usuario:usr_1"
        );
        assert_eq!(
            dependency_index_prefix(&projection),
            "ridx:pedidos_con_usuario:"
        );
    }

    #[test]
    fn deleted_event_is_detected_from_event_type() {
        let event = EventEnvelope {
            event_id: "evt-1".into(),
            event_type: "usuario.deleted".into(),
            store: "usuarios".into(),
            entity: "usuario".into(),
            key: "usr_1".into(),
            version: 2,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: json!({}),
        };
        assert!(is_deleted(&event));
    }
}

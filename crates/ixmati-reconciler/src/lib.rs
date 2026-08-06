use ixmati_cache::CacheClient;
use ixmati_core::ProjectionRegistry;
use rusqlite::Connection;
use std::collections::HashMap;

pub struct ReconcilerReport {
    pub total_projections: usize,
    pub reprojected: usize,
    pub entities_scanned: usize,
    pub dry_run_skipped: usize,
    pub errors: Vec<String>,
}

impl Default for ReconcilerReport {
    fn default() -> Self {
        Self {
            total_projections: 0,
            reprojected: 0,
            entities_scanned: 0,
            dry_run_skipped: 0,
            errors: Vec::new(),
        }
    }
}

pub struct ReconcilerConfig {
    pub projection_filter: Option<String>,
    pub dry_run: bool,
    pub store_paths: HashMap<String, String>,
}

pub struct Reconciler {
    registry: ProjectionRegistry,
}

impl Reconciler {
    pub fn new(registry: ProjectionRegistry) -> Self {
        Self { registry }
    }

    pub async fn reconcile_async(
        &self,
        config: &ReconcilerConfig,
        cache: &CacheClient,
    ) -> ReconcilerReport {
        let projections = if let Some(ref name) = config.projection_filter {
            self.registry
                .projections()
                .iter()
                .filter(|p| p.name == *name)
                .collect::<Vec<_>>()
        } else {
            self.registry.projections().iter().collect()
        };

        let mut report = ReconcilerReport {
            total_projections: projections.len(),
            ..Default::default()
        };

        for proj in &projections {
            if config.dry_run {
                report.dry_run_skipped += 1;
                continue;
            }

            match self.rebuild_projection(proj, config, cache).await {
                Ok(count) => {
                    report.reprojected += 1;
                    report.entities_scanned += count;
                    tracing::info!(
                        projection = %proj.name,
                        entities = count,
                        "projection rebuilt"
                    );
                }
                Err(e) => {
                    report.errors.push(format!("{}: {}", proj.name, e));
                    tracing::error!(projection = %proj.name, error = %e, "reconcile failed");
                }
            }
        }

        report
    }

    async fn rebuild_projection(
        &self,
        proj: &ixmati_core::ProjectionConfig,
        config: &ReconcilerConfig,
        cache: &CacheClient,
    ) -> Result<usize, String> {
        use ixmati_core::ProjectionPattern;

        match proj.pattern {
            ProjectionPattern::R => self.rebuild_pattern_r(proj, config, cache).await,
            ProjectionPattern::M => self.rebuild_pattern_m(proj, config, cache).await,
        }
    }

    async fn rebuild_pattern_r(
        &self,
        proj: &ixmati_core::ProjectionConfig,
        config: &ReconcilerConfig,
        cache: &CacheClient,
    ) -> Result<usize, String> {
        let mut store_data: HashMap<String, HashMap<String, serde_json::Value>> = HashMap::new();

        for store_name in &proj.source_stores {
            let db_path = config
                .store_paths
                .get(store_name)
                .ok_or_else(|| format!("no db_path for store '{}'", store_name))?;

            let conn = Connection::open(db_path)
                .map_err(|e| format!("open {}: {}", db_path, e))?;

            let table = format!("payload_{}", store_name);
            let sql = format!("SELECT entity, key, payload FROM {}", table);

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| format!("prepare query on {}: {}", store_name, e))?;

            let rows: Vec<(String, String, Vec<u8>)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                })
                .map_err(|e| format!("query {}: {}", store_name, e))?
                .filter_map(|r| r.ok())
                .collect();

            let mut entities: HashMap<String, serde_json::Value> = HashMap::new();
            for (_entity, key, payload_bytes) in rows {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&payload_bytes) {
                    entities.insert(key, value);
                }
            }
            store_data.insert(store_name.clone(), entities);
        }

        let primary_store = &proj.source_stores[0];
        let primary_data = store_data
            .get(primary_store)
            .ok_or_else(|| format!("no data for primary store '{}'", primary_store))?;

        let mut count = 0usize;

        for (key, payload) in primary_data {
            let mut projection_payload = serde_json::Map::new();
            projection_payload.insert(primary_store.clone(), payload.clone());

            for other_store in proj.source_stores.iter().skip(1) {
                if let Some(other_data) = store_data.get(other_store) {
                    let lookup_key = event_key_from_value(
                        &payload,
                        &proj.target_key,
                        key,
                    );
                    if let Some(other_payload) = other_data.get(&lookup_key) {
                        projection_payload.insert(other_store.clone(), other_payload.clone());
                    }
                }
            }

            let projection_key = event_key_from_value(&payload, &proj.target_key, key);
            let merged = serde_json::Value::Object(projection_payload);
            let merged_bytes = serde_json::to_vec(&merged).unwrap_or_default();

            cache
                .set_projection(&proj.name, &projection_key, &merged_bytes)
                .await;
            count += 1;
        }

        Ok(count)
    }

    async fn rebuild_pattern_m(
        &self,
        proj: &ixmati_core::ProjectionConfig,
        config: &ReconcilerConfig,
        cache: &CacheClient,
    ) -> Result<usize, String> {
        let copy_fields = proj
            .copy_fields
            .as_ref()
            .ok_or_else(|| "pattern M requires copy_fields".to_string())?;

        let mut count = 0usize;

        for field_def in copy_fields {
            let db_path = config
                .store_paths
                .get(&field_def.source_store)
                .ok_or_else(|| {
                    format!("no db_path for store '{}'", field_def.source_store)
                })?;

            let conn = Connection::open(db_path)
                .map_err(|e| format!("open {}: {}", db_path, e))?;

            let table = format!("payload_{}", field_def.source_store);
            let sql = format!(
                "SELECT entity, key, payload FROM {} WHERE entity = ?1",
                table
            );

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| format!("prepare query on {}: {}", field_def.source_store, e))?;

            let rows: Vec<(String, String, Vec<u8>)> = stmt
                .query_map([&field_def.source_entity], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                })
                .map_err(|e| format!("query {}: {}", field_def.source_store, e))?
                .filter_map(|r| r.ok())
                .collect();

            for (_entity, key, payload_bytes) in rows {
                let payload: serde_json::Value =
                    serde_json::from_slice(&payload_bytes).unwrap_or_default();

                let mut projection_payload = serde_json::Map::new();
                let target_key = event_key_from_value(&payload, &proj.target_key, &key);

                for field_name in &field_def.fields {
                    if let Some(value) = payload.get(field_name) {
                        projection_payload.insert(field_name.clone(), value.clone());
                    }
                }

                if projection_payload.is_empty() {
                    continue;
                }

                projection_payload.insert(
                    proj.target_key.clone(),
                    serde_json::Value::String(target_key.clone()),
                );

                let merged = serde_json::Value::Object(projection_payload);
                let merged_bytes = serde_json::to_vec(&merged).unwrap_or_default();

                cache
                    .set_projection(&proj.name, &target_key, &merged_bytes)
                    .await;
                count += 1;
            }
        }

        Ok(count)
    }
}

fn event_key_from_value(payload: &serde_json::Value, target_key: &str, fallback: &str) -> String {
    if let Some(val) = payload.get(target_key) {
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
        return val.to_string();
    }
    fallback.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixmati_core::{ProjectionConfig, ProjectionPattern};

    #[test]
    fn event_key_from_value_extracts_field() {
        let payload = serde_json::json!({"pedido_id": "abc123", "total": 100});
        assert_eq!(
            event_key_from_value(&payload, "pedido_id", "fallback"),
            "abc123"
        );
    }

    #[test]
    fn event_key_from_value_falls_back() {
        let payload = serde_json::json!({"total": 100});
        assert_eq!(
            event_key_from_value(&payload, "pedido_id", "fallback"),
            "fallback"
        );
    }

    #[test]
    fn reconciler_config_defaults() {
        let config = ReconcilerConfig {
            projection_filter: None,
            dry_run: true,
            store_paths: HashMap::new(),
        };
        assert!(config.dry_run);
        assert!(config.projection_filter.is_none());
    }

    #[test]
    fn reconciler_new_stores_registry() {
        let proj = ProjectionConfig {
            name: "test_prj".into(),
            pattern: ProjectionPattern::R,
            source_stores: vec!["pedidos".into()],
            target_key: "k".into(),
            ttl_seconds: 300,
            copy_fields: None,
        };
        let registry = ProjectionRegistry::new(vec![proj]);
        let reconciler = Reconciler::new(registry);
        assert_eq!(reconciler.registry.len(), 1);
    }
}

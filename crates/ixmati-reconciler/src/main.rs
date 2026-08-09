use ixmati_cache::CacheClient;
use ixmati_core::{ProjectionConfig, ProjectionRegistry};
use ixmati_reconciler::{Reconciler, ReconcilerConfig};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(serde::Deserialize)]
struct ProjectionsFile {
    projections: Vec<ProjectionConfig>,
}

fn load_projections(path: &str) -> ProjectionRegistry {
    match std::fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<ProjectionsFile>(&content) {
            Ok(file) => {
                let registry = ProjectionRegistry::new(file.projections);
                match registry.validate() {
                    Ok(()) => {
                        tracing::info!(count = registry.len(), path = %path, "projections loaded");
                    }
                    Err(errors) => {
                        tracing::error!(?errors, "projection validation errors");
                        std::process::exit(1);
                    }
                }
                registry
            }
            Err(e) => {
                tracing::error!(error = %e, path = %path, "failed to parse projections file");
                std::process::exit(1);
            }
        },
        Err(e) => {
            tracing::error!(error = %e, path = %path, "failed to read projections file");
            std::process::exit(1);
        }
    }
}

fn parse_store_paths(val: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in val.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        if let Some((name, path)) = pair.split_once('=') {
            map.insert(name.trim().to_string(), path.trim().to_string());
        } else {
            tracing::warn!(pair = %pair, "invalid store path format, expected name=path");
        }
    }
    map
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_reconciler=info".into()),
        )
        .json()
        .init();

    let socket_path = std::env::var("CACHE_SOCKET_PATH")
        .unwrap_or_else(|_| "/var/run/ixmati/cache.sock".into());
    let projections_path = std::env::var("IXMATI_PROJECTIONS_PATH")
        .unwrap_or_else(|_| "config/projections.toml".into());
    let stores_env = std::env::var("IXMATI_STORE_PATHS").unwrap_or_default();
    let projection_filter = std::env::var("IXMATI_PROJECTION_FILTER").ok();
    let dry_run = std::env::var("IXMATI_DRY_RUN")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    let store_paths = parse_store_paths(&stores_env);

    tracing::info!(
        socket = %socket_path,
        projections = %projections_path,
        stores = ?store_paths.keys().collect::<Vec<_>>(),
        filter = ?projection_filter,
        dry_run,
        "ixmati-reconciler starting"
    );

    if store_paths.is_empty() {
        tracing::error!(
            "IXMATI_STORE_PATHS required (format: store1=/path/db1,store2=/path/db2)"
        );
        std::process::exit(1);
    }

    let registry = load_projections(&projections_path);

    let cache = match CacheClient::connect(&socket_path).await {
        Ok(c) => Arc::new(c),
        Err(e) => {
            tracing::error!(error = %e, socket = %socket_path, "failed to connect to cache-server");
            std::process::exit(1);
        }
    };

    let reconciler = Reconciler::new(registry);

    let config = ReconcilerConfig {
        projection_filter,
        dry_run,
        store_paths,
    };

    let report = reconciler.reconcile_async(&config, &cache).await;

    tracing::info!(
        total = report.total_projections,
        reprojected = report.reprojected,
        scanned = report.entities_scanned,
        skipped = report.dry_run_skipped,
        errors = report.errors.len(),
        "reconcile complete"
    );

    for error in &report.errors {
        tracing::error!(%error, "reconcile error");
    }

    if !report.errors.is_empty() {
        std::process::exit(1);
    }
}

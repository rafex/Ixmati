use ixmati_cache::NoOpBackend;
use ixmati_core::ProjectionRegistry;
use ixmati_reconciler::{Reconciler, ReconcilerConfig};
use std::sync::Arc;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ixmati_reconciler=info".into()),
        )
        .json()
        .init();

    let registry = ProjectionRegistry::new(vec![]);
    let cache = Arc::new(NoOpBackend::new());
    let reconciler = Reconciler::new(registry, cache);

    let config = ReconcilerConfig {
        projection_filter: None,
        dry_run: false,
    };

    let report = reconciler.reconcile(&config);
    tracing::info!(
        total = report.total_projections,
        reprojected = report.reprojected,
        "reconcile complete"
    );
}

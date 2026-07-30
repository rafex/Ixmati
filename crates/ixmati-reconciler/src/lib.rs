use ixmati_core::ProjectionRegistry;
use ixmati_cache::CacheBackend;
use std::sync::Arc;

pub struct Reconciler<B: CacheBackend> {
    registry: ProjectionRegistry,
    cache: Arc<B>,
}

pub struct ReconcilerConfig {
    pub projection_filter: Option<String>,
    pub dry_run: bool,
}

impl<B: CacheBackend> Reconciler<B> {
    pub fn new(registry: ProjectionRegistry, cache: Arc<B>) -> Self {
        Self { registry, cache }
    }

    pub fn reconcile(&self, config: &ReconcilerConfig) -> ReconcilerReport {
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

        for _proj in &projections {
            if config.dry_run {
                report.dry_run_skipped += 1;
                continue;
            }

            report.reprojected += 1;
        }

        report
    }
}

#[derive(Debug, Default)]
pub struct ReconcilerReport {
    pub total_projections: usize,
    pub reprojected: usize,
    pub dry_run_skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixmati_cache::NoOpBackend;
    use ixmati_core::{ProjectionConfig, ProjectionPattern};

    #[test]
    fn reconciler_processes_all_projections() {
        let proj = ProjectionConfig {
            name: "test_prj".into(),
            pattern: ProjectionPattern::R,
            source_stores: vec!["pedidos".into()],
            target_key: "k".into(),
            ttl_seconds: 300,
            copy_fields: None,
        };

        let registry = ProjectionRegistry::new(vec![proj]);
        let cache = Arc::new(NoOpBackend::new());
        let reconciler = Reconciler::new(registry, cache);

        let report = reconciler.reconcile(&ReconcilerConfig {
            projection_filter: None,
            dry_run: false,
        });

        assert_eq!(report.total_projections, 1);
        assert_eq!(report.reprojected, 1);
        assert_eq!(report.dry_run_skipped, 0);
    }

    #[test]
    fn reconciler_dry_run_skips_all() {
        let proj = ProjectionConfig {
            name: "test_prj".into(),
            pattern: ProjectionPattern::R,
            source_stores: vec!["pedidos".into()],
            target_key: "k".into(),
            ttl_seconds: 300,
            copy_fields: None,
        };

        let registry = ProjectionRegistry::new(vec![proj]);
        let cache = Arc::new(NoOpBackend::new());
        let reconciler = Reconciler::new(registry, cache);

        let report = reconciler.reconcile(&ReconcilerConfig {
            projection_filter: None,
            dry_run: true,
        });

        assert_eq!(report.total_projections, 1);
        assert_eq!(report.reprojected, 0);
        assert_eq!(report.dry_run_skipped, 1);
    }

    #[test]
    fn reconciler_filter_by_name() {
        let proj_a = ProjectionConfig {
            name: "prj_a".into(),
            pattern: ProjectionPattern::R,
            source_stores: vec!["pedidos".into()],
            target_key: "k".into(),
            ttl_seconds: 300,
            copy_fields: None,
        };
        let proj_b = ProjectionConfig {
            name: "prj_b".into(),
            pattern: ProjectionPattern::M,
            source_stores: vec!["usuarios".into()],
            target_key: "k".into(),
            ttl_seconds: 600,
            copy_fields: Some(vec![]),
        };

        let registry = ProjectionRegistry::new(vec![proj_a, proj_b]);
        let cache = Arc::new(NoOpBackend::new());
        let reconciler = Reconciler::new(registry, cache);

        let report = reconciler.reconcile(&ReconcilerConfig {
            projection_filter: Some("prj_a".into()),
            dry_run: false,
        });

        assert_eq!(report.total_projections, 1);
        assert_eq!(report.reprojected, 1);
    }
}

//! Backpressure basada en el backlog real del outbox (ver DEC-0043/DEC-0044):
//! el rate-limiter de `throttle.rs` limita cuántas escrituras se aceptan por
//! ventana de tiempo, pero no sabe si el writer está drenando el outbox al
//! mismo ritmo. Este módulo mantiene el último `OUTBOX_SIZE` conocido por
//! store (poblado por `self_monitor.rs`) y permite rechazar escrituras
//! cuando ese backlog supera un umbral configurable.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

pub struct OutboxBacklog {
    threshold: i64,
    depths: RwLock<HashMap<String, i64>>,
}

impl OutboxBacklog {
    /// `threshold <= 0` deshabilita la backpressure (siempre permite).
    pub fn new(threshold: i64) -> Self {
        Self {
            threshold,
            depths: RwLock::new(HashMap::new()),
        }
    }

    pub fn threshold(&self) -> i64 {
        self.threshold
    }

    pub fn update(&self, store: &str, depth: i64) {
        if let Ok(mut map) = self.depths.write() {
            map.insert(store.to_string(), depth);
        }
    }

    /// Pone en 0 cualquier store previamente conocido que no aparezca en
    /// `seen` (outbox ya drenado en el último poll) y devuelve los stores
    /// que efectivamente cambiaron, para que el caller también pueda
    /// reflejarlo en la métrica OUTBOX_SIZE.
    pub fn reset_missing(&self, seen: &HashSet<String>) -> Vec<String> {
        let mut reset = Vec::new();
        if let Ok(mut map) = self.depths.write() {
            for (store, depth) in map.iter_mut() {
                if !seen.contains(store) && *depth != 0 {
                    *depth = 0;
                    reset.push(store.clone());
                }
            }
        }
        reset
    }

    /// `Some(depth)` si el store está por encima del umbral configurado.
    pub fn check(&self, store: &str) -> Option<i64> {
        if self.threshold <= 0 {
            return None;
        }
        let map = self.depths.read().ok()?;
        map.get(store).copied().filter(|&d| d > self.threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_threshold_is_none() {
        let bp = OutboxBacklog::new(100);
        bp.update("default", 50);
        assert_eq!(bp.check("default"), None);
    }

    #[test]
    fn over_threshold_returns_depth() {
        let bp = OutboxBacklog::new(100);
        bp.update("default", 150);
        assert_eq!(bp.check("default"), Some(150));
    }

    #[test]
    fn unknown_store_is_none() {
        let bp = OutboxBacklog::new(100);
        assert_eq!(bp.check("unknown"), None);
    }

    #[test]
    fn zero_threshold_disables_backpressure() {
        let bp = OutboxBacklog::new(0);
        bp.update("default", 999_999);
        assert_eq!(bp.check("default"), None);
    }

    #[test]
    fn reset_missing_zeroes_drained_stores_and_reports_them() {
        let bp = OutboxBacklog::new(100);
        bp.update("default", 500);
        bp.update("other", 10);

        let seen: HashSet<String> = ["other".to_string()].into_iter().collect();
        let reset = bp.reset_missing(&seen);

        assert_eq!(reset, vec!["default".to_string()]);
        assert_eq!(bp.check("default"), None);
    }

    #[test]
    fn reset_missing_is_idempotent_for_already_zero_stores() {
        let bp = OutboxBacklog::new(100);
        bp.update("default", 0);

        let reset = bp.reset_missing(&HashSet::new());
        assert!(reset.is_empty());
    }
}

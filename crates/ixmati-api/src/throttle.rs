use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_BURST: usize = 3;

#[derive(Clone, Copy)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

#[derive(Clone)]
pub struct Throttle {
    refill_per_second: f64,
    burst: f64,
    buckets: HashMap<String, Bucket>,
}

impl Throttle {
    pub fn new(max_writes_per_window: usize, window_secs: u64) -> Self {
        let window = Duration::from_secs(window_secs.max(1));
        let refill_per_second = max_writes_per_window as f64 / window.as_secs_f64();
        let burst = max_writes_per_window.min(MAX_BURST) as f64;
        Self {
            refill_per_second,
            burst,
            buckets: HashMap::new(),
        }
    }

    pub fn allow(&mut self, store: &str) -> bool {
        self.allow_at(store, Instant::now())
    }

    fn allow_at(&mut self, store: &str, now: Instant) -> bool {
        let bucket = self.buckets.entry(store.to_string()).or_insert(Bucket {
            tokens: self.burst,
            last_refill: now,
        });

        let elapsed = now.saturating_duration_since(bucket.last_refill);
        bucket.tokens =
            (bucket.tokens + elapsed.as_secs_f64() * self.refill_per_second).min(self.burst);
        bucket.last_refill = now;

        if bucket.tokens < 1.0 {
            return false;
        }

        bucket.tokens -= 1.0;
        true
    }

    pub fn depth(&self, store: &str) -> usize {
        self.buckets
            .get(store)
            .map(|bucket| (self.burst - bucket.tokens).ceil().max(0.0) as usize)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::Throttle;
    use std::time::{Duration, Instant};

    #[test]
    fn steady_rate_at_configured_limit_absorbs_scheduler_jitter() {
        let mut throttle = Throttle::new(30, 1);
        let start = Instant::now();
        let mut accepted = 0;

        for index in 0..300 {
            let jitter = if index % 3 == 0 { 2 } else { 0 };
            let at = start + Duration::from_millis(index * 34 + jitter);
            accepted += throttle.allow_at("pedidos", at) as usize;
        }

        assert_eq!(accepted, 300);
    }

    #[test]
    fn burst_is_bounded_and_refills_at_configured_rate() {
        let mut throttle = Throttle::new(30, 1);
        let start = Instant::now();

        assert!(throttle.allow_at("pedidos", start));
        assert!(throttle.allow_at("pedidos", start));
        assert!(throttle.allow_at("pedidos", start));
        assert!(!throttle.allow_at("pedidos", start));

        assert!(throttle.allow_at("pedidos", start + Duration::from_millis(34)));
    }

    #[test]
    fn stores_have_independent_buckets() {
        let mut throttle = Throttle::new(1, 1);
        let start = Instant::now();

        assert!(throttle.allow_at("pedidos", start));
        assert!(!throttle.allow_at("pedidos", start));
        assert!(throttle.allow_at("usuarios", start));
    }
}

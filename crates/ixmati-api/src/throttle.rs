use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Throttle {
    max_writes: usize,
    window: Duration,
    windows: HashMap<String, VecDeque<Instant>>,
}

impl Throttle {
    pub fn new(max_writes_per_window: usize, window_secs: u64) -> Self {
        Self {
            max_writes: max_writes_per_window,
            window: Duration::from_secs(window_secs),
            windows: HashMap::new(),
        }
    }

    pub fn allow(&mut self, store: &str) -> bool {
        let now = Instant::now();
        let entries = self.windows.entry(store.to_string()).or_default();

        while entries.front().is_some_and(|t| now - *t > self.window) {
            entries.pop_front();
        }

        if entries.len() >= self.max_writes {
            return false;
        }

        entries.push_back(now);
        true
    }

    pub fn depth(&self, store: &str) -> usize {
        self.windows.get(store).map(|e| e.len()).unwrap_or(0)
    }
}

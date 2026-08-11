//! Optional safety net for a writer that receives commands but stops making
//! durable progress. It is opt-in because a timeout is a recovery policy,
//! not proof of the MQTT root cause.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub struct Progress {
    pending_since_ms: AtomicU64,
}

impl Progress {
    pub fn command_received(&self) {
        let now = now_ms();
        let _ =
            self.pending_since_ms
                .compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
    }

    pub fn batch_committed(&self) {
        self.pending_since_ms.store(0, Ordering::Relaxed);
    }
}

pub fn spawn(progress: Arc<Progress>, timeout: Duration) {
    if timeout.is_zero() {
        tracing::info!("MQTT progress watchdog disabled (MQTT_WATCHDOG_TIMEOUT_MS=0)");
        return;
    }

    std::thread::Builder::new()
        .name("ixmati-watchdog".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            let pending_since = progress.pending_since_ms.load(Ordering::Relaxed);
            if pending_since == 0 {
                continue;
            }
            let elapsed = Duration::from_millis(now_ms().saturating_sub(pending_since));
            if should_exit(pending_since, now_ms(), timeout) {
                tracing::error!(
                    elapsed_ms = elapsed.as_millis(),
                    timeout_ms = timeout.as_millis(),
                    "writer has pending commands but no durable progress; exiting for systemd recovery"
                );
                std::process::exit(42);
            }
        })
        .expect("failed to spawn writer progress watchdog");
}

fn should_exit(pending_since_ms: u64, now_ms: u64, timeout: Duration) -> bool {
    pending_since_ms != 0
        && Duration::from_millis(now_ms.saturating_sub(pending_since_ms)) >= timeout
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchdog_ignores_empty_progress() {
        assert!(!should_exit(0, 10_000, Duration::from_secs(1)));
    }

    #[test]
    fn watchdog_waits_until_timeout() {
        assert!(!should_exit(10_000, 10_999, Duration::from_secs(1)));
        assert!(should_exit(10_000, 11_000, Duration::from_secs(1)));
    }

    #[test]
    fn watchdog_handles_clock_regression_without_false_positive() {
        assert!(!should_exit(10_000, 9_000, Duration::from_secs(1)));
    }
}

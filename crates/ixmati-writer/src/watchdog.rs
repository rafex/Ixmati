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
            if elapsed >= timeout {
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

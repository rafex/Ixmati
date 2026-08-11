//! Periodic self-instrumentation: process RSS/CPU (via /proc, Linux only —
//! the only platform Ixmati ships to) and outbox backlog per store.

use std::sync::atomic::{AtomicU64, Ordering};

// Standard on Linux unless the kernel was built with a non-default
// CONFIG_HZ; reading it via sysconf would need a libc dependency this
// crate doesn't otherwise have, so we accept the same assumption
// virtually every /proc-based Prometheus exporter makes.
const CLK_TCK: u64 = 100;

static LAST_UTIME_TICKS: AtomicU64 = AtomicU64::new(0);

pub fn spawn(
    db_path: Option<String>,
    db_paths: std::collections::HashMap<String, String>,
    outbox_backlog: std::sync::Arc<crate::backpressure::OutboxBacklog>,
) {
    let paths = if let Some(path) = db_path {
        vec![path]
    } else {
        db_paths.into_values().collect()
    };
    tokio::spawn(async move {
        loop {
            collect_process_metrics();
            for path in &paths {
                collect_outbox_metrics(path, &outbox_backlog);
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

fn collect_process_metrics() {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                if let Some(kb) = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                {
                    crate::metrics::PROCESS_MEMORY_RSS.record(kb * 1024, &[]);
                }
                break;
            }
        }
    }

    if let Ok(stat) = std::fs::read_to_string("/proc/self/stat") {
        // Field layout: "pid (comm) state ppid ...". comm can contain
        // spaces/parens, so split on the last ')' rather than whitespace.
        if let Some(paren_end) = stat.rfind(')') {
            let fields: Vec<&str> = stat[paren_end + 1..].split_whitespace().collect();
            // After state(0) ppid(1) ... the 12th field here is utime
            // (field 14 in `man 5 proc`, 0-indexed from state=field 3).
            if let Some(utime) = fields.get(11).and_then(|v| v.parse::<u64>().ok()) {
                let prev = LAST_UTIME_TICKS.swap(utime, Ordering::Relaxed);
                if utime > prev {
                    let delta_secs = (utime - prev) as f64 / CLK_TCK as f64;
                    crate::metrics::PROCESS_CPU_USER.add(delta_secs, &[]);
                }
            }
        }
    }
}

fn collect_outbox_metrics(db_path: &str, outbox_backlog: &crate::backpressure::OutboxBacklog) {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut stmt = match conn
        .prepare("SELECT store, COUNT(*) FROM _outbox WHERE published_at IS NULL GROUP BY store")
    {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    });
    let Ok(rows) = rows else { return };

    let mut seen = std::collections::HashSet::new();
    for row in rows.flatten() {
        let (store, count) = row;
        seen.insert(store.clone());
        crate::metrics::OUTBOX_SIZE.record(count, &crate::metrics::kv_store(&store));
        outbox_backlog.update(&store, count);
    }

    // Stores conocidos previamente que ya no aparecen en el resultado (0
    // filas sin publicar) no salen en el GROUP BY — sin esto, tanto la
    // métrica como la backpressure quedarían con el último valor != 0
    // stale para siempre una vez que el writer drena el backlog.
    for store in outbox_backlog.reset_missing(&seen) {
        crate::metrics::OUTBOX_SIZE.record(0, &crate::metrics::kv_store(&store));
    }
}

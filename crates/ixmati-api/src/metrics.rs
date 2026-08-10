use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, HistogramVec, IntCounterVec, IntGaugeVec, Registry,
    register_counter_with_registry, register_gauge_with_registry,
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry,
};
use std::sync::LazyLock;

pub static REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    Registry::new_custom(Some("ixmati".into()), None).expect("failed to create metrics registry")
});

pub static WRITE_REQUESTS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec_with_registry!(
        "ixmati_write_requests_total",
        "Total write requests received",
        &["store", "entity", "status"],
        REGISTRY
    )
    .expect("failed to register write_requests_total")
});

pub static WRITE_LATENCY: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec_with_registry!(
        "ixmati_write_latency_seconds",
        "Write request latency in seconds",
        &["store"],
        vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0
        ],
        REGISTRY
    )
    .expect("failed to register write_latency_seconds")
});

pub static QUEUE_DEPTH: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    register_int_gauge_vec_with_registry!(
        "ixmati_queue_depth",
        "Current command queue depth per store",
        &["store"],
        REGISTRY
    )
    .expect("failed to register queue_depth")
});

pub static OUTBOX_SIZE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    register_int_gauge_vec_with_registry!(
        "ixmati_outbox_size",
        "Unpublished events in outbox per store",
        &["store"],
        REGISTRY
    )
    .expect("failed to register outbox_size")
});

pub static PROJECTION_LAG: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    register_int_gauge_vec_with_registry!(
        "ixmati_projection_lag_events",
        "Number of events behind for projection",
        &["projection", "store"],
        REGISTRY
    )
    .expect("failed to register projection_lag_events")
});

pub static WRITE_ERRORS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec_with_registry!(
        "ixmati_write_errors_total",
        "Total write errors",
        &["store", "error_type"],
        REGISTRY
    )
    .expect("failed to register write_errors_total")
});

pub static CACHE_HITS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec_with_registry!(
        "ixmati_cache_hits_total",
        "Total cache hits",
        &["store", "namespace"],
        REGISTRY
    )
    .expect("failed to register cache_hits_total")
});

pub static CACHE_MISSES: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec_with_registry!(
        "ixmati_cache_misses_total",
        "Total cache misses",
        &["store", "namespace"],
        REGISTRY
    )
    .expect("failed to register cache_misses_total")
});

pub static PROCESS_MEMORY_RSS: LazyLock<Gauge> = LazyLock::new(|| {
    register_gauge_with_registry!(
        "ixmati_process_memory_rss_bytes",
        "Process resident set size in bytes",
        REGISTRY
    )
    .expect("failed to register process_memory_rss_bytes")
});

pub static PROCESS_CPU_USER: LazyLock<Counter> = LazyLock::new(|| {
    register_counter_with_registry!(
        "ixmati_process_cpu_user_seconds_total",
        "Process user CPU time in seconds",
        REGISTRY
    )
    .expect("failed to register process_cpu_user_seconds_total")
});

pub static WRITE_BATCH_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec_with_registry!(
        "ixmati_write_batch_duration_seconds",
        "Time to process a batch of writes (seconds)",
        &["store"],
        vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0
        ],
        REGISTRY
    )
    .expect("failed to register write_batch_duration_seconds")
});

pub fn encode_metrics() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = Vec::new();
    encoder
        .encode(&REGISTRY.gather(), &mut buffer)
        .expect("failed to encode metrics");
    String::from_utf8(buffer).expect("metrics not valid utf8")
}

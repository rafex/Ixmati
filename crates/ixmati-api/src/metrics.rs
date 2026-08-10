use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter, MeterProvider};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use std::sync::LazyLock;

// prometheus::Registry backing the OTel Prometheus exporter. encode_metrics()
// gathers from this registry, not from the OTel SDK directly.
pub static PROM_REGISTRY: LazyLock<prometheus::Registry> = LazyLock::new(prometheus::Registry::new);

// Kept alive for the process lifetime: dropping it would tear down the
// metrics pipeline the instruments below write into.
static METER_PROVIDER: LazyLock<SdkMeterProvider> = LazyLock::new(|| {
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(PROM_REGISTRY.clone())
        .with_namespace("ixmati")
        .build()
        .expect("failed to build OpenTelemetry Prometheus exporter");
    SdkMeterProvider::builder().with_reader(exporter).build()
});

static METER: LazyLock<Meter> = LazyLock::new(|| METER_PROVIDER.meter("ixmati-api"));

// Counter names omit the `_total` suffix: the Prometheus exporter appends it
// automatically (namespace "ixmati" + name "write_requests" -> the observed
// series is ixmati_write_requests_total). Adding it here would double it up.

pub static WRITE_REQUESTS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("write_requests")
        .with_description("Total write requests received")
        .build()
});

pub static WRITE_LATENCY: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("write_latency_seconds")
        .with_description("Write request latency in seconds")
        .with_boundaries(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ])
        .build()
});

pub static QUEUE_DEPTH: LazyLock<Gauge<i64>> = LazyLock::new(|| {
    METER
        .i64_gauge("queue_depth")
        .with_description("Current command queue depth per store")
        .build()
});

pub static OUTBOX_SIZE: LazyLock<Gauge<i64>> = LazyLock::new(|| {
    METER
        .i64_gauge("outbox_size")
        .with_description("Unpublished events in outbox per store")
        .build()
});

pub static PROJECTION_LAG: LazyLock<Gauge<i64>> = LazyLock::new(|| {
    METER
        .i64_gauge("projection_lag_events")
        .with_description("Number of events behind for projection")
        .build()
});

pub static WRITE_ERRORS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("write_errors")
        .with_description("Total write errors")
        .build()
});

pub static CACHE_HITS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("cache_hits")
        .with_description("Total cache hits")
        .build()
});

pub static CACHE_MISSES: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("cache_misses")
        .with_description("Total cache misses")
        .build()
});

pub static PROCESS_MEMORY_RSS: LazyLock<Gauge<u64>> = LazyLock::new(|| {
    METER
        .u64_gauge("process_memory_rss_bytes")
        .with_description("Process resident set size in bytes")
        .build()
});

pub static PROCESS_CPU_USER: LazyLock<Counter<f64>> = LazyLock::new(|| {
    METER
        .f64_counter("process_cpu_user_seconds")
        .with_description("Process user CPU time in seconds")
        .build()
});

pub static WRITE_BATCH_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("write_batch_duration_seconds")
        .with_description("Time to process a batch of writes (seconds)")
        .with_boundaries(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ])
        .build()
});

pub fn kv_store(store: &str) -> [KeyValue; 1] {
    [KeyValue::new("store", store.to_string())]
}

pub fn encode_metrics() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = Vec::new();
    encoder
        .encode(&PROM_REGISTRY.gather(), &mut buffer)
        .expect("failed to encode metrics");
    String::from_utf8(buffer).expect("metrics not valid utf8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_metrics_has_single_ixmati_prefix_and_no_prometheus_dep_leak() {
        WRITE_REQUESTS.add(
            3,
            &[
                KeyValue::new("store", "default"),
                KeyValue::new("entity", "test"),
                KeyValue::new("status", "success"),
            ],
        );
        CACHE_HITS.add(4, &kv_store("default"));
        QUEUE_DEPTH.record(7, &kv_store("default"));
        WRITE_LATENCY.record(0.012, &kv_store("default"));

        let out = encode_metrics();
        println!("{out}");

        assert!(out.contains("ixmati_write_requests_total"));
        assert!(!out.contains("ixmati_ixmati_"));
        assert!(!out.contains("_total_total"));
        assert!(out.contains("ixmati_cache_hits_total"));
        assert!(out.contains("ixmati_queue_depth"));
        assert!(out.contains("ixmati_write_latency_seconds_bucket"));
    }
}

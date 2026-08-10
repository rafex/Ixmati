//! Métricas OTel del writer (ver DEC-0043/DEC-0044/DEC-0047).
//!
//! Exposición HTTP opt-in vía `METRICS_PORT`: sin esa env var, las métricas
//! se siguen registrando en memoria pero no hay endpoint `/metrics` que
//! escuchar. Es a propósito — `ixmati-writer` corre como unidad systemd
//! template (`ixmati-writer@<store>`), una instancia por store; un puerto
//! por defecto fijo colisionaría entre instancias del mismo host. El
//! operador que quiera scrapear cada store por separado debe asignar un
//! `METRICS_PORT` único por instancia (drop-in de systemd).

use opentelemetry::metrics::{Gauge, Histogram, Meter, MeterProvider};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use std::sync::LazyLock;

pub static PROM_REGISTRY: LazyLock<prometheus::Registry> = LazyLock::new(prometheus::Registry::new);

static METER_PROVIDER: LazyLock<SdkMeterProvider> = LazyLock::new(|| {
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(PROM_REGISTRY.clone())
        .with_namespace("ixmati")
        .build()
        .expect("failed to build OpenTelemetry Prometheus exporter");
    SdkMeterProvider::builder().with_reader(exporter).build()
});

static METER: LazyLock<Meter> = LazyLock::new(|| METER_PROVIDER.meter("ixmati-writer"));

pub static WRITE_BATCH_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("write_batch_duration_seconds")
        .with_description("Time to process a batch of writes (seconds)")
        .with_boundaries(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ])
        .build()
});

// Ver DEC-0048/TASK-VAL-0016: antes de esto, el canal entre el eventloop de
// MQTT y el loop principal era `mpsc::unbounded_channel()` — sin límite ni
// visibilidad. Ahora es acotado y se observa su profundidad acá.
pub static CONSUMER_QUEUE_DEPTH: LazyLock<Gauge<u64>> = LazyLock::new(|| {
    METER
        .u64_gauge("consumer_queue_depth")
        .with_description("Commands buffered between the MQTT eventloop and the batcher")
        .build()
});

pub fn encode_metrics() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = Vec::new();
    encoder
        .encode(&PROM_REGISTRY.gather(), &mut buffer)
        .expect("failed to encode metrics");
    String::from_utf8(buffer).expect("metrics not valid utf8")
}

/// Arranca el servidor HTTP `/metrics` si `METRICS_PORT` está seteado.
/// No hace nada (y no falla) si no lo está.
pub fn maybe_spawn_server() {
    let Ok(port) = std::env::var("METRICS_PORT") else {
        return;
    };
    let Ok(port) = port.parse::<u16>() else {
        tracing::warn!(value = %port, "METRICS_PORT inválido, ignorando");
        return;
    };

    tokio::spawn(async move {
        let app = axum::Router::new().route(
            "/metrics",
            axum::routing::get(|| async { encode_metrics() }),
        );
        let addr = format!("0.0.0.0:{port}");
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                tracing::info!(%addr, "writer metrics endpoint listening");
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!(error = %e, "metrics server error");
                }
            }
            Err(e) => {
                tracing::error!(error = %e, %addr, "failed to bind metrics endpoint");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_metrics_reports_write_batch_duration() {
        WRITE_BATCH_DURATION.record(0.02, &[opentelemetry::KeyValue::new("store", "default")]);
        let out = encode_metrics();
        assert!(out.contains("ixmati_write_batch_duration_seconds_bucket"));
        assert!(!out.contains("ixmati_ixmati_"));
    }
}

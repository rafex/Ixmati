//! Métricas OTel del projector (ver DEC-0043/DEC-0047).
//!
//! `PROJECTION_LAG` mide lag de *procesamiento* (ahora - `occurred_at` del
//! evento), no lag en cantidad de eventos: el projector es un consumidor
//! puramente basado en stream (MQTT `ixmati/evt/#`), no tiene visibilidad
//! del total de eventos publicados de forma independiente de lo que ya
//! recibió — no hay un "offset" contra el que comparar como en un log
//! particionado. El lag de tiempo es la señal operacional real y accionable
//! (el ROADMAP ya lo usa como criterio de alerta: "lag de proyección > 1s").
//!
//! Exposición HTTP opt-in vía `METRICS_PORT` (mismo patrón que
//! `ixmati-writer`, ver ese módulo para el porqué).

use opentelemetry::metrics::{Histogram, Meter, MeterProvider};
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

static METER: LazyLock<Meter> = LazyLock::new(|| METER_PROVIDER.meter("ixmati-projector"));

pub static PROJECTION_LAG: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("projection_lag_seconds")
        .with_description("Delay between event occurred_at and projector processing time")
        .with_boundaries(vec![
            0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
        ])
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

/// Registra el lag de procesamiento de un evento a partir de su
/// `occurred_at` (RFC3339). Silenciosamente no hace nada si el timestamp no
/// parsea — un evento con timestamp inválido no debe tumbar el pipeline.
pub fn record_lag(occurred_at: &str) {
    let Ok(occurred) = chrono::DateTime::parse_from_rfc3339(occurred_at) else {
        return;
    };
    let lag_secs = (chrono::Utc::now() - occurred.with_timezone(&chrono::Utc)).num_milliseconds()
        as f64
        / 1000.0;
    if lag_secs >= 0.0 {
        PROJECTION_LAG.record(lag_secs, &[]);
    }
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
                tracing::info!(%addr, "projector metrics endpoint listening");
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
    fn record_lag_ignores_invalid_timestamp() {
        record_lag("not-a-timestamp");
        // No panic = éxito; nada que aserter sobre el estado del histograma
        // sin acoplarse al output exacto del encoder.
    }

    #[test]
    fn record_lag_accepts_past_timestamp_and_reports_it() {
        record_lag("2020-01-01T00:00:00Z");
        let out = encode_metrics();
        assert!(out.contains("ixmati_projection_lag_seconds_bucket"));
        assert!(!out.contains("ixmati_ixmati_"));
    }
}

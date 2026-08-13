//! Métricas OTel del writer (ver DEC-0043/DEC-0044/DEC-0047).
//!
//! Exposición HTTP opt-in vía `METRICS_PORT`: sin esa env var, las métricas
//! se siguen registrando en memoria pero no hay endpoint `/metrics` que
//! escuchar. Es a propósito — `ixmati-writer` corre como unidad systemd
//! template (`ixmati-writer@<store>`), una instancia por store; un puerto
//! por defecto fijo colisionaría entre instancias del mismo host. El
//! operador que quiera scrapear cada store por separado debe asignar un
//! `METRICS_PORT` único por instancia (drop-in de systemd).

use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter, MeterProvider};
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

pub static WRITE_THREAD_QUEUE_WAIT: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("write_thread_queue_wait_seconds")
        .with_description("Time a batch waits before the dedicated SQLite thread starts it")
        .with_boundaries(vec![
            0.0001, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
        ])
        .build()
});

pub static SQLITE_PROCESS_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("sqlite_process_duration_seconds")
        .with_description("Time spent processing a batch in SQLite")
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

// Mapeo de puntos calientes (ver conversación TASK-VAL-0018): WRITE_BATCH_DURATION
// ya aísla el costo de SQLite (~250ms/batch medido, contra 6-25ms en
// benchmarks aislados sin el resto de la tubería). Esta métrica aísla el
// otro tramo del ciclo — la llamada a `cache_sync.sync_batch()` contra el
// cache-server real — para confirmar o descartar de una vez si ahí está el
// tiempo que ninguno de los benchmarks sintéticos (que no incluyen esta
// llamada) logró reproducir.
// DEC-0055/TASK-VAL-0024: antes un ack fallido solo dejaba un
// `tracing::warn!` que se pierde entre miles de líneas bajo carga — este
// contador lo hace visible sin tener que grepear logs. Un fallo de ack
// persistente (crece sin parar) es la señal más temprana de que la sesión
// MQTT puede estar por atascarse (ver el hallazgo del atasco de sesión en
// DEC-0055).
pub static MQTT_ACK_FAILURES: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("mqtt_ack_failures")
        .with_description("MQTT publish acks that failed to send")
        .build()
});

pub static MQTT_CONSUMER_CONNECTED: LazyLock<Gauge<i64>> = LazyLock::new(|| {
    METER
        .i64_gauge("mqtt_consumer_connected")
        .with_description("Whether the writer MQTT consumer is connected")
        .build()
});

pub static MQTT_CONSUMER_LAST_EVENT_UNIX_SECONDS: LazyLock<Gauge<i64>> = LazyLock::new(|| {
    METER
        .i64_gauge("mqtt_consumer_last_event_unix_seconds")
        .with_description("Unix timestamp of the last command received by the MQTT consumer")
        .build()
});

pub static MQTT_CONSUMER_LAST_ACK_UNIX_SECONDS: LazyLock<Gauge<i64>> = LazyLock::new(|| {
    METER
        .i64_gauge("mqtt_consumer_last_ack_unix_seconds")
        .with_description("Unix timestamp of the last command acknowledged by the writer")
        .build()
});

pub static MQTT_EVENTLOOP_ERRORS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("mqtt_eventloop_errors")
        .with_description("MQTT event loop errors")
        .build()
});

pub static MQTT_COMMANDS_ENQUEUED: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("mqtt_commands_enqueued")
        .with_description("Valid MQTT commands accepted into the bounded writer queue")
        .build()
});

pub static MQTT_COMMANDS_DEFERRED: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("mqtt_commands_deferred")
        .with_description("Valid MQTT commands deferred because the writer queue was full")
        .build()
});

pub static MQTT_COMMANDS_ACKED: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("mqtt_commands_acked")
        .with_description("MQTT commands acknowledged after a successful SQLite commit")
        .build()
});

pub static CACHE_SYNC_ERRORS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("cache_sync_errors")
        .with_description("Cache synchronization operations that failed after SQLite commit")
        .build()
});

pub static CACHE_SYNC_DEFERRED: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("cache_sync_deferred")
        .with_description("Post-commit cache batches deferred because the bounded queue was full")
        .build()
});

pub static CACHE_SYNC_QUEUE_DEPTH: LazyLock<Gauge<u64>> = LazyLock::new(|| {
    METER
        .u64_gauge("cache_sync_queue_depth")
        .with_description("Post-commit cache batches waiting for synchronization")
        .build()
});

pub static LAST_BATCH_COMMIT_UNIX_SECONDS: LazyLock<Gauge<i64>> = LazyLock::new(|| {
    METER
        .i64_gauge("last_batch_commit_unix_seconds")
        .with_description("Unix timestamp of the last successfully committed SQLite batch")
        .build()
});

pub static CACHE_SYNC_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("cache_sync_duration_seconds")
        .with_description("Time to sync a batch of writes to the cache-server")
        .with_boundaries(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ])
        .build()
});

pub static BATCH_ACK_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("batch_ack_duration_seconds")
        .with_description("Time spent acknowledging commands after SQLite commit")
        .with_boundaries(vec![
            0.0001, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
        ])
        .build()
});

pub static BATCH_CYCLE_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("batch_cycle_duration_seconds")
        .with_description(
            "Time from batch fill start through durable commit and projection enqueue",
        )
        .with_boundaries(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ])
        .build()
});

pub static OUTBOX_PUBLISH_ATTEMPTS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("outbox_publish_attempts")
        .with_description("Outbox rows submitted to MQTT")
        .build()
});

pub static OUTBOX_PUBACK_TIMEOUTS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("outbox_puback_timeouts")
        .with_description("Outbox rows that did not receive PUBACK before timeout")
        .build()
});

pub static OUTBOX_PUBLISHED: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("outbox_published")
        .with_description("Outbox rows marked published after a PUBACK")
        .build()
});

pub static OUTBOX_MARK_FAILURES: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("outbox_mark_failures")
        .with_description("Failed attempts to mark PUBACKed outbox rows")
        .build()
});

pub static OUTBOX_PUBACK_QUEUE_DROPS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    METER
        .u64_counter("outbox_puback_queue_drops")
        .with_description("PUBACKs dropped because the bounded acknowledgement queue was full")
        .build()
});

pub static OUTBOX_MARK_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("outbox_mark_duration_seconds")
        .with_description("Time to mark PUBACKed outbox rows as published")
        .with_boundaries(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ])
        .build()
});

pub static OUTBOX_LAST_PUBACK_UNIX_SECONDS: LazyLock<Gauge<i64>> = LazyLock::new(|| {
    METER
        .i64_gauge("outbox_last_puback_unix_seconds")
        .with_description("Unix timestamp of the last outbox PUBACK")
        .build()
});

// WRITE_BATCH_DURATION + CACHE_SYNC_DURATION juntas no explican el ciclo
// completo medido en producción (~187ms de ~454ms observados por batch,
// dejando ~268ms/batch sin atribuir) — a diferencia de
// `bench_channel_loop.rs`, que reproduce el mismo canal/select!/interval
// pero SIN el resto de la tubería real (eventloop de MQTT, servidor de
// métricas) compitiendo por el runtime. Esta métrica aísla el tiempo de
// "llenar" un batch (recibir hasta 100 comandos vía `recv()`) bajo carga
// real, para ver si ahí está el resto.
pub static BATCH_FILL_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    METER
        .f64_histogram("batch_fill_duration_seconds")
        .with_description("Time spent accumulating a batch before it's flushed")
        .with_boundaries(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
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

/// Arranca el servidor HTTP `/metrics` si `METRICS_PORT` está seteado.
/// No hace nada (y no falla) si no lo está.
///
/// DEC-0052: el proceso `ixmati-writer` ya no corre dentro de un runtime
/// tokio compartido (`main.rs` es `fn main()` normal, sin `#[tokio::main]`).
/// `axum`/`tokio::net::TcpListener` siguen necesitando un runtime para
/// funcionar, así que este hilo se construye el suyo propio, confinado por
/// completo a sí mismo — igual que el patrón ya usado para las conexiones
/// MQTT síncronas de `rumqttc` (`consumer.rs`/`event_publisher.rs`). Este
/// runtime nunca toca SQLite ni comparte primitivas con el hilo de
/// escritura: exponer métricas no es parte del camino crítico.
pub fn maybe_spawn_server() {
    let Ok(port) = std::env::var("METRICS_PORT") else {
        return;
    };
    let Ok(port) = port.parse::<u16>() else {
        tracing::warn!(value = %port, "METRICS_PORT inválido, ignorando");
        return;
    };

    std::thread::Builder::new()
        .name("ixmati-metrics".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build metrics server runtime");

            rt.block_on(async move {
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
        })
        .expect("failed to spawn metrics server thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_metrics_reports_write_batch_duration() {
        WRITE_BATCH_DURATION.record(0.02, &[opentelemetry::KeyValue::new("store", "default")]);
        MQTT_ACK_FAILURES.add(1, &[]);
        MQTT_COMMANDS_ENQUEUED.add(1, &[]);
        MQTT_COMMANDS_DEFERRED.add(1, &[]);
        MQTT_COMMANDS_ACKED.add(1, &[]);
        CACHE_SYNC_ERRORS.add(1, &[]);
        CACHE_SYNC_DEFERRED.add(1, &[]);
        OUTBOX_PUBACK_TIMEOUTS.add(1, &[]);
        OUTBOX_PUBLISHED.add(1, &[]);
        OUTBOX_MARK_FAILURES.add(1, &[]);
        OUTBOX_PUBACK_QUEUE_DROPS.add(1, &[]);
        OUTBOX_MARK_DURATION.record(0.01, &[]);
        let out = encode_metrics();
        assert!(out.contains("ixmati_write_batch_duration_seconds_bucket"));
        assert!(out.contains("ixmati_mqtt_ack_failures_total"));
        assert!(out.contains("ixmati_mqtt_commands_enqueued_total"));
        assert!(out.contains("ixmati_mqtt_commands_deferred_total"));
        assert!(out.contains("ixmati_mqtt_commands_acked_total"));
        assert!(out.contains("ixmati_cache_sync_errors_total"));
        assert!(out.contains("ixmati_outbox_puback_timeouts_total"));
        assert!(out.contains("ixmati_outbox_published_total"));
        assert!(out.contains("ixmati_outbox_mark_failures_total"));
        assert!(out.contains("ixmati_outbox_puback_queue_drops_total"));
        assert!(out.contains("ixmati_outbox_mark_duration_seconds_bucket"));
        assert!(!out.contains("ixmati_ixmati_"));
        assert!(!out.contains("_total_total"));
    }
}

//! Rate-controlled protocol benchmark for JSON REST, binary REST and gRPC.
//!
//! The client keeps only a bounded latency reservoir and reports whether the
//! requested rate was actually achievable at the configured concurrency.

#![allow(deprecated)]

use ixmati_api::grpc::pb;
use prost::Message;
use prost_types::{Struct, Value, value::Kind};
use reqwest::Client;
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tonic::{Request, transport::Channel};

const RESERVOIR_SIZE: usize = 100_000;

#[derive(Clone)]
struct Config {
    protocol: String,
    url: String,
    grpc_url: String,
    rate: f64,
    duration: Duration,
    warmup: Duration,
    cooldown: Duration,
    concurrency: usize,
    api_key: String,
    store: String,
    entity: String,
}

#[derive(Debug)]
struct Sample {
    latency_ms: f64,
    class: String,
    error: Option<String>,
}

#[derive(Default)]
struct Reservoir {
    values: Vec<f64>,
    seen: u64,
    state: u64,
}

impl Reservoir {
    fn push(&mut self, value: f64) {
        self.seen += 1;
        if self.values.len() < RESERVOIR_SIZE {
            self.values.push(value);
            return;
        }
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let index = (self.state % self.seen) as usize;
        if index < RESERVOIR_SIZE {
            self.values[index] = value;
        }
    }

    fn percentile(&self, percentile: f64) -> f64 {
        if self.values.is_empty() {
            return 0.0;
        }
        let mut values = self.values.clone();
        values.sort_by(f64::total_cmp);
        let index = ((percentile / 100.0) * values.len() as f64).ceil() as usize;
        values[index.saturating_sub(1).min(values.len() - 1)]
    }
}

#[derive(Serialize)]
struct Report {
    protocol: String,
    target: String,
    target_rate: f64,
    duration_seconds: f64,
    warmup_seconds: f64,
    cooldown_seconds: f64,
    concurrency: usize,
    submitted_operations: u64,
    completed_operations: u64,
    successful_operations: u64,
    throughput_per_second: f64,
    client_saturated_ticks: u64,
    valid_rate_controlled: bool,
    responses: BTreeMap<String, u64>,
    errors: BTreeMap<String, u64>,
    latency_ms: LatencyReport,
}

#[derive(Serialize)]
struct LatencyReport {
    p50: f64,
    p95: f64,
    p99: f64,
    max: f64,
}

fn arg(args: &[String], name: &str, default: &str) -> String {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].clone())
        .unwrap_or_else(|| default.into())
}

fn parse_config() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let protocol = arg(&args, "--protocol", "json");
    assert!(
        matches!(protocol.as_str(), "json" | "protobuf" | "grpc"),
        "--protocol must be json, protobuf or grpc"
    );
    let rate: f64 = arg(&args, "--rate", "40")
        .parse()
        .expect("--rate must be a positive number");
    assert!(rate > 0.0, "--rate must be positive");
    Config {
        protocol,
        url: arg(&args, "--url", "http://127.0.0.1:30000"),
        grpc_url: arg(&args, "--grpc-url", "http://127.0.0.1:30100"),
        rate,
        duration: Duration::from_secs(
            arg(&args, "--duration", "30")
                .parse()
                .expect("--duration must be an integer"),
        ),
        warmup: Duration::from_secs(
            arg(&args, "--warmup", "5")
                .parse()
                .expect("--warmup must be an integer"),
        ),
        cooldown: Duration::from_secs(
            arg(&args, "--cooldown", "2")
                .parse()
                .expect("--cooldown must be an integer"),
        ),
        concurrency: arg(&args, "--concurrency", "64")
            .parse()
            .expect("--concurrency must be an integer"),
        api_key: arg(&args, "--api-key", "ix-default-key"),
        store: arg(&args, "--store", "pedidos"),
        entity: arg(&args, "--entity", "pedido"),
    }
}

fn payload(key: &str) -> Struct {
    Struct {
        fields: BTreeMap::from([
            (
                "pedido_id".into(),
                Value {
                    kind: Some(Kind::StringValue(key.into())),
                },
            ),
            (
                "usuario_id".into(),
                Value {
                    kind: Some(Kind::StringValue("usr_000001".into())),
                },
            ),
            (
                "total".into(),
                Value {
                    kind: Some(Kind::NumberValue(42.5)),
                },
            ),
        ]),
    }
}

fn request_message(config: &Config, sequence: u64) -> pb::WriteRequest {
    let key = format!("protocol-bench-{sequence}");
    pb::WriteRequest {
        envelope: Some(pb::WriteEnvelope {
            op: "upsert".into(),
            store: config.store.clone(),
            entity: config.entity.clone(),
            key: key.clone(),
            version: 1,
            ts: "2026-08-12T00:00:00Z".into(),
            idempotency_key: format!("protocol-bench-idem-{sequence}"),
            ack_mode: "committed".into(),
            payload: Some(payload(&key)),
            payload_bytes: Vec::new(),
        }),
    }
}

async fn http_request(client: Client, config: Config, sequence: u64) -> Sample {
    let started = Instant::now();
    let message = request_message(&config, sequence);
    let response = if config.protocol == "protobuf" {
        client
            .post(format!("{}/write", config.url.trim_end_matches('/')))
            .header("Authorization", format!("ApiKey {}", config.api_key))
            .header("Content-Type", "application/protobuf")
            .body(message.encode_to_vec())
            .send()
            .await
    } else {
        let envelope = message.envelope.expect("benchmark envelope");
        let payload = serde_json::json!({
            "op": envelope.op,
            "store": envelope.store,
            "entity": envelope.entity,
            "key": envelope.key,
            "version": envelope.version,
            "ts": envelope.ts,
            "idempotency_key": envelope.idempotency_key,
            "ack_mode": envelope.ack_mode,
            "payload": {"pedido_id": format!("protocol-bench-{sequence}"), "usuario_id": "usr_000001", "total": 42.5}
        });
        client
            .post(format!("{}/write", config.url.trim_end_matches('/')))
            .header("Authorization", format!("ApiKey {}", config.api_key))
            .json(&payload)
            .send()
            .await
    };
    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
    match response {
        Ok(response) => {
            let status = response.status();
            let _ = response.bytes().await;
            let class = status.as_u16().to_string();
            Sample {
                latency_ms,
                class: class.clone(),
                error: (!status.is_success()).then_some(format!("http_{class}")),
            }
        }
        Err(error) => Sample {
            latency_ms,
            class: "transport_error".into(),
            error: Some(error.to_string()),
        },
    }
}

async fn grpc_request(channel: Channel, config: Config, sequence: u64) -> Sample {
    let started = Instant::now();
    let mut client = pb::write_service_client::WriteServiceClient::new(channel);
    let mut request = Request::new(request_message(&config, sequence));
    if let Ok(value) = config.api_key.parse() {
        request.metadata_mut().insert("x-api-key", value);
    }
    let result = client.write(request).await;
    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
    match result {
        Ok(response) => {
            let status = response.into_inner().status;
            Sample {
                latency_ms,
                class: status.clone(),
                error: (!matches!(status.as_str(), "COMMITTED" | "PENDING")).then_some(status),
            }
        }
        Err(error) => Sample {
            latency_ms,
            class: format!("grpc_{:?}", error.code()),
            error: Some(error.code().to_string()),
        },
    }
}

async fn run_window(config: &Config, record: bool) -> (Vec<Sample>, u64, u64) {
    let client = Client::builder()
        .pool_max_idle_per_host(config.concurrency)
        .build()
        .expect("HTTP client");
    let channel = if config.protocol == "grpc" {
        Some(
            Channel::from_shared(config.grpc_url.clone())
                .expect("valid --grpc-url")
                .connect()
                .await
                .expect("connect gRPC endpoint"),
        )
    } else {
        None
    };
    let deadline = Instant::now() + config.duration;
    let interval = Duration::from_secs_f64(1.0 / config.rate);
    let mut next_submit = Instant::now();
    let mut sequence = 0;
    let mut submitted = 0;
    let mut saturated = 0;
    let mut tasks = JoinSet::new();
    let mut samples = Vec::new();

    while Instant::now() < deadline {
        while let Some(result) = tasks.try_join_next() {
            if record {
                samples.push(result.expect("benchmark task"));
            }
        }
        if tasks.len() >= config.concurrency {
            saturated += 1;
            if let Some(result) = tasks.join_next().await
                && record
            {
                samples.push(result.expect("benchmark task"));
            }
            continue;
        }
        let now = Instant::now();
        if next_submit > now {
            tokio::time::sleep_until(tokio::time::Instant::from_std(next_submit)).await;
        }
        let request_config = config.clone();
        let request_client = client.clone();
        let request_channel = channel.clone();
        tasks.spawn(async move {
            if request_config.protocol == "grpc" {
                grpc_request(
                    request_channel.expect("gRPC channel"),
                    request_config,
                    sequence,
                )
                .await
            } else {
                http_request(request_client, request_config, sequence).await
            }
        });
        sequence += 1;
        submitted += 1;
        next_submit += interval;
    }
    while let Some(result) = tasks.join_next().await {
        if record {
            samples.push(result.expect("benchmark task"));
        }
    }
    (samples, submitted, saturated)
}

#[tokio::main]
async fn main() {
    let config = parse_config();
    if config.warmup > Duration::ZERO {
        let mut warmup = config.clone();
        warmup.duration = config.warmup;
        let _ = run_window(&warmup, false).await;
        tokio::time::sleep(config.cooldown).await;
    }
    let (samples, submitted, saturated) = run_window(&config, true).await;
    let mut reservoir = Reservoir::default();
    let mut responses = BTreeMap::new();
    let mut errors = BTreeMap::new();
    let mut successful = 0;
    for sample in samples {
        reservoir.push(sample.latency_ms);
        *responses.entry(sample.class).or_insert(0) += 1;
        if let Some(error) = sample.error {
            *errors.entry(error).or_insert(0) += 1;
        } else {
            successful += 1;
        }
    }
    let completed = responses.values().sum::<u64>();
    let report = Report {
        protocol: config.protocol.clone(),
        target: if config.protocol == "grpc" {
            config.grpc_url
        } else {
            config.url
        },
        target_rate: config.rate,
        duration_seconds: config.duration.as_secs_f64(),
        warmup_seconds: config.warmup.as_secs_f64(),
        cooldown_seconds: config.cooldown.as_secs_f64(),
        concurrency: config.concurrency,
        submitted_operations: submitted,
        completed_operations: completed,
        successful_operations: successful,
        throughput_per_second: successful as f64 / config.duration.as_secs_f64().max(0.001),
        client_saturated_ticks: saturated,
        valid_rate_controlled: saturated == 0,
        responses,
        errors,
        latency_ms: LatencyReport {
            p50: reservoir.percentile(50.0),
            p95: reservoir.percentile(95.0),
            p99: reservoir.percentile(99.0),
            max: reservoir.values.iter().copied().fold(0.0, f64::max),
        },
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("serialize report")
    );
}

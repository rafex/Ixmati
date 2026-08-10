use crate::cache_client::CacheClient;
use crate::cache_proxy::CacheProxy;
use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::Json,
    routing::{get, post},
};
use ixmati_cache::CacheBackend;
use ixmati_core::{AckResponse, WriteEnvelope};
use opentelemetry::KeyValue;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

pub enum CacheReadMode {
    Direct,
    Mqtt(CacheProxy),
    Socket(Arc<CacheClient>),
}

impl Clone for CacheReadMode {
    fn clone(&self) -> Self {
        match self {
            Self::Direct => Self::Direct,
            Self::Mqtt(p) => Self::Mqtt(p.clone()),
            Self::Socket(c) => Self::Socket(Arc::clone(c)),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    mqtt_broker: String,
    db_path: Option<String>,
    mqtt_client: Arc<Mutex<Option<AsyncClient>>>,
    throttle: Arc<Mutex<crate::throttle::Throttle>>,
    cache: Arc<dyn CacheBackend>,
    cache_read_mode: CacheReadMode,
    cache_proxy_client: Arc<Mutex<Option<AsyncClient>>>,
    outbox_backlog: Arc<crate::backpressure::OutboxBacklog>,
}

fn outbox_backpressure_threshold_from_env() -> i64 {
    std::env::var("OUTBOX_BACKPRESSURE_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000)
}

impl AppState {
    pub fn new(
        broker: &str,
        cache: Arc<dyn CacheBackend>,
        cache_proxy: Option<CacheProxy>,
        proxy_client: Option<AsyncClient>,
        cache_socket: Option<Arc<CacheClient>>,
    ) -> Self {
        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);
        let (client, mut eventloop) = AsyncClient::new(
            MqttOptions::new(format!("api-{}", Uuid::new_v4()), &host, port),
            100,
        );

        tokio::spawn(async move {
            loop {
                if eventloop.poll().await.is_err() {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        });

        let max_writes: usize = std::env::var("MAX_WRITES_PER_WINDOW")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);
        let window_secs: u64 = std::env::var("THROTTLE_WINDOW_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        let cache_read_mode = if let Some(proxy) = cache_proxy {
            CacheReadMode::Mqtt(proxy)
        } else if let Some(socket) = cache_socket {
            CacheReadMode::Socket(socket)
        } else {
            CacheReadMode::Direct
        };

        Self {
            mqtt_broker: broker.to_string(),
            db_path: None,
            mqtt_client: Arc::new(Mutex::new(Some(client))),
            throttle: Arc::new(Mutex::new(crate::throttle::Throttle::new(
                max_writes,
                window_secs,
            ))),
            cache,
            cache_read_mode,
            cache_proxy_client: Arc::new(Mutex::new(proxy_client)),
            outbox_backlog: Arc::new(crate::backpressure::OutboxBacklog::new(
                outbox_backpressure_threshold_from_env(),
            )),
        }
    }

    pub fn with_db(
        broker: &str,
        db_path: &str,
        cache: Arc<dyn CacheBackend>,
        cache_socket: Option<Arc<CacheClient>>,
        cache_read_mode: CacheReadMode,
        cache_proxy_client: Arc<Mutex<Option<AsyncClient>>>,
    ) -> Self {
        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);
        let (client, mut eventloop) = AsyncClient::new(
            MqttOptions::new(format!("api-{}", Uuid::new_v4()), &host, port),
            100,
        );

        tokio::spawn(async move {
            loop {
                if eventloop.poll().await.is_err() {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        });

        let max_writes: usize = std::env::var("MAX_WRITES_PER_WINDOW")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);
        let window_secs: u64 = std::env::var("THROTTLE_WINDOW_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        let cache_read_mode = if let Some(socket) = cache_socket {
            CacheReadMode::Socket(socket)
        } else {
            cache_read_mode
        };

        Self {
            mqtt_broker: broker.to_string(),
            db_path: Some(db_path.to_string()),
            mqtt_client: Arc::new(Mutex::new(Some(client))),
            throttle: Arc::new(Mutex::new(crate::throttle::Throttle::new(
                max_writes,
                window_secs,
            ))),
            cache,
            cache_read_mode,
            cache_proxy_client,
            outbox_backlog: Arc::new(crate::backpressure::OutboxBacklog::new(
                outbox_backpressure_threshold_from_env(),
            )),
        }
    }

    pub fn broker(&self) -> &str {
        &self.mqtt_broker
    }

    pub fn outbox_backlog(&self) -> Arc<crate::backpressure::OutboxBacklog> {
        Arc::clone(&self.outbox_backlog)
    }
}

pub fn routes(state: AppState, auth_config: crate::auth::AuthConfig) -> Router {
    let protected = Router::new()
        .route("/write", post(write_handler))
        .route_layer(middleware::from_fn_with_state(
            auth_config,
            crate::auth::require_auth,
        ));

    let public = Router::new()
        .route(
            "/writes/{store}/{idempotency_key}",
            get(write_status_handler),
        )
        .route("/read", get(read_handler))
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler));

    Router::new()
        .merge(protected)
        .merge(public)
        .with_state(state)
}

async fn write_handler(
    State(state): State<AppState>,
    Json(envelope): Json<WriteEnvelope>,
) -> Result<Json<AckResponse>, (StatusCode, Json<ixmati_core::Error>)> {
    let started = std::time::Instant::now();
    let store = envelope.store.clone();
    let entity = envelope.entity.clone();

    let record_error = |error_type: &str| {
        crate::metrics::WRITE_ERRORS.add(
            1,
            &[
                KeyValue::new("store", store.clone()),
                KeyValue::new("error_type", error_type.to_string()),
            ],
        );
        crate::metrics::WRITE_LATENCY.record(started.elapsed().as_secs_f64(), &crate::metrics::kv_store(&store));
    };

    // Backpressure real (DEC-0043/DEC-0044): a diferencia del throttle de
    // abajo (limita cuántas escrituras se ACEPTAN por ventana), esto mira
    // si el writer está DRENANDO el outbox al mismo ritmo. Umbral vía
    // OUTBOX_BACKPRESSURE_THRESHOLD, deshabilitado si es <= 0.
    if let Some(depth) = state.outbox_backlog.check(&envelope.store) {
        let threshold = state.outbox_backlog.threshold();
        tracing::warn!(
            store = %envelope.store,
            depth,
            threshold,
            "rejecting write: outbox backlog over threshold"
        );
        record_error("outbox_backlog");
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(ixmati_core::Error::OutboxBacklog {
                store: envelope.store.clone(),
                depth,
                threshold,
            }),
        ));
    }

    {
        let mut throttle = state.throttle.lock().await;
        if !throttle.allow(&envelope.store) {
            let depth = throttle.depth(&envelope.store);
            crate::metrics::QUEUE_DEPTH
                .record(depth as i64, &crate::metrics::kv_store(&envelope.store));
            record_error("queue_full");
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(ixmati_core::Error::QueueFull {
                    store: envelope.store.clone(),
                }),
            ));
        }
        let depth = throttle.depth(&envelope.store);
        crate::metrics::QUEUE_DEPTH.record(depth as i64, &crate::metrics::kv_store(&envelope.store));
    }

    let idempotency_key = if envelope.idempotency_key.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        envelope.idempotency_key.clone()
    };

    let cmd = WriteEnvelope {
        idempotency_key: idempotency_key.clone(),
        ..envelope
    };

    let topic = format!("ixmati/cmd/{}/{}/{}", cmd.store, cmd.entity, cmd.key);

    let payload = serde_json::to_vec(&cmd).map_err(|e| {
        record_error("serialize");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ixmati_core::Error::Internal {
                detail: e.to_string(),
            }),
        )
    })?;

    let guard = state.mqtt_client.lock().await;
    let client = guard.as_ref().ok_or_else(|| {
        record_error("mqtt_unavailable");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ixmati_core::Error::Internal {
                detail: "MQTT client not available".into(),
            }),
        )
    })?;

    client
        .publish(&topic, QoS::AtLeastOnce, false, payload)
        .await
        .map_err(|e| {
            record_error("mqtt_publish");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ixmati_core::Error::Internal {
                    detail: e.to_string(),
                }),
            )
        })?;
    drop(guard);

    crate::metrics::WRITE_REQUESTS.add(
        1,
        &[
            KeyValue::new("store", cmd.store.clone()),
            KeyValue::new("entity", entity),
            KeyValue::new("status", "success"),
        ],
    );
    crate::metrics::WRITE_LATENCY.record(started.elapsed().as_secs_f64(), &crate::metrics::kv_store(&cmd.store));

    let ack = match cmd.ack_mode.as_str() {
        "committed" => AckResponse {
            status: "ACCEPTED".into(),
            store: cmd.store.clone(),
            idempotency_key,
            message: Some("published, waiting commit".into()),
        },
        _ => AckResponse {
            status: "ACCEPTED".into(),
            store: cmd.store.clone(),
            idempotency_key,
            message: Some("published".into()),
        },
    };

    Ok(Json(ack))
}

#[derive(Deserialize)]
struct ReadQuery {
    #[allow(dead_code)]
    store: Option<String>,
    #[allow(dead_code)]
    entity: Option<String>,
    #[allow(dead_code)]
    key: Option<String>,
    #[allow(dead_code)]
    projection: Option<String>,
}

async fn read_handler(
    State(state): State<AppState>,
    Query(params): Query<ReadQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ixmati_core::Error>)> {
    if let Some(ref proj) = params.projection {
        let key = params.key.as_deref().unwrap_or("");
        let pkey = format!("p:{proj}:{key}");

        let cached: Option<Vec<u8>> = match &state.cache_read_mode {
            CacheReadMode::Socket(client) => client.get_raw(&pkey).await,
            CacheReadMode::Mqtt(proxy) => {
                let client_guard = state.cache_proxy_client.lock().await;
                if let Some(client) = client_guard.as_ref() {
                    proxy.query(client, "projections", &pkey, "").await
                } else {
                    None
                }
            }
            CacheReadMode::Direct => state.cache.get("projections", &pkey, ""),
        };

        if let Some(data) = cached {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) {
                crate::metrics::CACHE_HITS.add(
                    1,
                    &[
                        opentelemetry::KeyValue::new("store", proj.to_string()),
                        opentelemetry::KeyValue::new("namespace", "projection"),
                    ],
                );
                return Ok(Json(serde_json::json!({
                    "found": true,
                    "projection": proj,
                    "key": key,
                    "payload": value,
                })));
            }
        }

        crate::metrics::CACHE_MISSES.add(
            1,
            &[
                opentelemetry::KeyValue::new("store", proj.to_string()),
                opentelemetry::KeyValue::new("namespace", "projection"),
            ],
        );
        return Ok(Json(serde_json::json!({
            "found": false,
            "projection": proj,
            "key": key,
            "message": "projection not found"
        })));
    }

    let store = params.store.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ixmati_core::Error::StoreNotFound { store: "".into() }),
        )
    })?;
    let entity = params.entity.unwrap_or_default();
    let key = params.key.unwrap_or_default();

    let cache_key = format!("c:{}:{}:{}", store, entity, key);

    let cached: Option<Vec<u8>> = match &state.cache_read_mode {
        CacheReadMode::Direct => state.cache.get("cache", &cache_key, ""),
        CacheReadMode::Mqtt(proxy) => {
            let client_guard = state.cache_proxy_client.lock().await;
            if let Some(client) = client_guard.as_ref() {
                proxy.query(client, &store, &entity, &key).await
            } else {
                None
            }
        }
        CacheReadMode::Socket(client) => client.get(&store, &entity, &key).await,
    };

    if let Some(cached) = cached {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&cached) {
            crate::metrics::CACHE_HITS.add(
                1,
                &[
                    opentelemetry::KeyValue::new("store", store.clone()),
                    opentelemetry::KeyValue::new("namespace", "cache"),
                ],
            );
            return Ok(Json(serde_json::json!({
                "found": true,
                "store": store,
                "entity": entity,
                "key": key,
                "source": "cache",
                "payload": value,
            })));
        }
    }

    crate::metrics::CACHE_MISSES.add(
        1,
        &[
            opentelemetry::KeyValue::new("store", store.clone()),
            opentelemetry::KeyValue::new("namespace", "cache"),
        ],
    );

    let db_path = state.db_path.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ixmati_core::Error::Internal {
                detail: "no database configured for reads".into(),
            }),
        )
    })?;

    let payload_bytes: Vec<u8> = {
        let conn = rusqlite::Connection::open(db_path).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ixmati_core::Error::Internal {
                    detail: format!("SQLite open: {}", e),
                }),
            )
        })?;

        let sql = format!(
            "SELECT payload FROM payload_{store} WHERE entity = ?1 AND key = ?2",
            store = store
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ixmati_core::Error::Internal {
                    detail: format!("SQL prepare: {}", e),
                }),
            )
        })?;

        let result: rusqlite::Result<Vec<u8>> =
            stmt.query_row((&entity, &key), |row| row.get(0));

        drop(stmt);

        match result {
            Ok(bytes) => bytes,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Ok(Json(serde_json::json!({
                    "found": false,
                    "store": store,
                    "entity": entity,
                    "key": key,
                    "message": "not found"
                })));
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ixmati_core::Error::Internal {
                        detail: format!("SQL query: {}", e),
                    }),
                ));
            }
        }
    };

    match &state.cache_read_mode {
        CacheReadMode::Socket(client) => {
            client.set(&store, &entity, &key, &payload_bytes).await;
        }
        CacheReadMode::Direct => {
            state.cache.set("cache", &cache_key, "", &payload_bytes);
        }
        CacheReadMode::Mqtt(_) => {}
    }

    let value: serde_json::Value =
        serde_json::from_slice(&payload_bytes).unwrap_or_default();
    Ok(Json(serde_json::json!({
        "found": true,
        "store": store,
        "entity": entity,
        "key": key,
        "source": "sqlite",
        "payload": value,
    })))
}

#[derive(Deserialize)]
struct WriteStatusParams {
    store: String,
    idempotency_key: String,
}

async fn write_status_handler(
    State(state): State<AppState>,
    Path(params): Path<WriteStatusParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ixmati_core::Error>)> {
    let db_path = state.db_path.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ixmati_core::Error::Internal {
                detail: "no database configured for status queries".into(),
            }),
        )
    })?;

    match crate::status::StatusQuery::query(db_path, &params.store, &params.idempotency_key) {
        Ok(status) => match status {
            crate::status::WriteStatus::Pending {
                store,
                idempotency_key,
            } => Ok(Json(serde_json::json!({
                "status": "PENDING",
                "store": store,
                "idempotency_key": idempotency_key,
            }))),
            crate::status::WriteStatus::Applied {
                idempotency_key,
                store,
                entity,
                key,
                version,
                applied_at,
            } => Ok(Json(serde_json::json!({
                "status": "APPLIED",
                "store": store,
                "entity": entity,
                "key": key,
                "version": version,
                "idempotency_key": idempotency_key,
                "applied_at": applied_at,
            }))),
        },
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ixmati_core::Error::Internal {
                detail: e.to_string(),
            }),
        )),
    }
}

async fn health_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let mut checker = crate::health::HealthChecker::new();

    if let Some(ref db) = state.db_path {
        checker = checker.with_db(db);
    }

    checker = checker.with_mqtt(&state.mqtt_broker);

    let status = checker.check();
    Json(serde_json::to_value(status).unwrap_or_default())
}

async fn metrics_handler() -> (StatusCode, String) {
    let encoded = crate::metrics::encode_metrics();
    (StatusCode::OK, encoded)
}

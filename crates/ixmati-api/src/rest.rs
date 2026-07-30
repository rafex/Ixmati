use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use ixmati_core::{AckResponse, WriteEnvelope};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    mqtt_broker: String,
}

impl AppState {
    pub fn new(broker: &str) -> Self {
        Self {
            mqtt_broker: broker.to_string(),
        }
    }

    pub fn broker(&self) -> &str {
        &self.mqtt_broker
    }
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/write", post(write_handler))
        .route("/writes/{store}/{idempotency_key}", get(write_status_handler))
        .route("/read", get(read_handler))
        .route("/health", get(health_handler))
        .with_state(state)
}

async fn write_handler(
    State(state): State<AppState>,
    Json(envelope): Json<WriteEnvelope>,
) -> Result<Json<AckResponse>, (StatusCode, Json<ixmati_core::Error>)> {
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ixmati_core::Error::Internal {
                detail: e.to_string(),
            }),
        )
    })?;

    let mut mqtt_options = MqttOptions::new(
        &format!("api-{}", Uuid::new_v4()),
        state.broker(),
        1883,
    );
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

    let (client, _eventloop) = AsyncClient::new(mqtt_options, 10);

    client
        .publish(&topic, QoS::AtLeastOnce, false, payload)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ixmati_core::Error::Internal {
                    detail: e.to_string(),
                }),
            )
        })?;

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
    store: Option<String>,
    entity: Option<String>,
    key: Option<String>,
    projection: Option<String>,
}

async fn read_handler(
    Query(params): Query<ReadQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ixmati_core::Error>)> {
    if params.projection.is_some() {
        return Ok(Json(serde_json::json!({
            "found": false,
            "message": "projections not yet implemented"
        })));
    }

    let store = params.store.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ixmati_core::Error::StoreNotFound {
                store: "".into(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "found": false,
        "store": store,
        "message": "cache-aside not yet implemented"
    })))
}

#[derive(Deserialize)]
struct WriteStatusParams {
    store: String,
    idempotency_key: String,
}

async fn write_status_handler(
    Path(params): Path<WriteStatusParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ixmati_core::Error>)> {
    Ok(Json(serde_json::json!({
        "status": "PENDING",
        "store": params.store,
        "idempotency_key": params.idempotency_key,
        "detail": "status query not yet implemented"
    })))
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "overall": "OK",
        "components": [
            {"name": "api", "status": "OK", "detail": "running"}
        ]
    }))
}

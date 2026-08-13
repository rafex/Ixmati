#![allow(deprecated)]

use crate::auth::AuthConfig;
use crate::rest::{self, AppState};
use ixmati_core::WriteEnvelope;
use prost_types::{Struct, Value as PbValue, value::Kind};
use std::collections::{BTreeMap, HashSet};
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, transport::Server};

pub mod pb {
    tonic::include_proto!("ixmati.v1");
}

pub(crate) const PROTOBUF_CONTENT_TYPE: &str = "application/protobuf";
const DEFAULT_GRPC_MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const DEFAULT_GRPC_MAX_CONCURRENT_STREAMS: u32 = 256;
const DEFAULT_EVENT_STREAM_BUFFER_CAPACITY: usize = 128;

fn grpc_max_message_bytes_from_env() -> usize {
    std::env::var("GRPC_MAX_MESSAGE_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_GRPC_MAX_MESSAGE_BYTES)
}

fn grpc_max_concurrent_streams_from_env() -> u32 {
    std::env::var("GRPC_MAX_CONCURRENT_STREAMS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_GRPC_MAX_CONCURRENT_STREAMS)
}

fn event_stream_buffer_capacity_from_env() -> usize {
    std::env::var("EVENT_STREAM_BUFFER_CAPACITY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        // One slot is always reserved for RESOURCE_EXHAUSTED so a slow
        // client receives a resumable cursor instead of a silent EOF.
        .filter(|value| *value >= 2)
        .unwrap_or(DEFAULT_EVENT_STREAM_BUFFER_CAPACITY)
}

pub(crate) fn read_response_from_json(
    value: serde_json::Value,
) -> Result<pb::ReadResponse, Status> {
    let found = value
        .get("found")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Ok(pb::ReadResponse {
        found,
        store: value
            .get("store")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .into(),
        entity: value
            .get("entity")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .into(),
        key: value
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .into(),
        version: value
            .get("version")
            .and_then(|v| v.as_u64())
            .unwrap_or_default(),
        payload_bytes: Vec::new(),
        payload: value.get("payload").map(json_to_struct).transpose()?,
        projection: value
            .get("projection")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .into(),
        source: value
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .into(),
    })
}

pub(crate) fn struct_to_json(value: Option<Struct>) -> Result<serde_json::Value, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("payload is required"))?;
    let mut object = serde_json::Map::new();
    for (key, value) in value.fields {
        object.insert(key, pb_value_to_json(value)?);
    }
    Ok(serde_json::Value::Object(object))
}

fn pb_value_to_json(value: PbValue) -> Result<serde_json::Value, Status> {
    match value.kind {
        Some(Kind::NullValue(_)) => Ok(serde_json::Value::Null),
        Some(Kind::NumberValue(value)) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| Status::invalid_argument("payload contains a non-finite number")),
        Some(Kind::StringValue(value)) => Ok(serde_json::Value::String(value)),
        Some(Kind::BoolValue(value)) => Ok(serde_json::Value::Bool(value)),
        Some(Kind::StructValue(value)) => struct_to_json(Some(value)),
        Some(Kind::ListValue(value)) => value
            .values
            .into_iter()
            .map(pb_value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        None => Err(Status::invalid_argument("payload contains an empty value")),
    }
}

pub(crate) fn json_to_struct(value: &serde_json::Value) -> Result<Struct, Status> {
    let object = value
        .as_object()
        .ok_or_else(|| Status::internal("stored payload is not a JSON object"))?;
    let fields = object
        .iter()
        .map(|(key, value)| Ok((key.clone(), json_to_pb_value(value)?)))
        .collect::<Result<BTreeMap<_, _>, Status>>()?;
    Ok(Struct { fields })
}

fn json_to_pb_value(value: &serde_json::Value) -> Result<PbValue, Status> {
    let kind = match value {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(value) => Kind::BoolValue(*value),
        serde_json::Value::Number(value) => Kind::NumberValue(
            value
                .as_f64()
                .ok_or_else(|| Status::internal("JSON number cannot be represented as f64"))?,
        ),
        serde_json::Value::String(value) => Kind::StringValue(value.clone()),
        serde_json::Value::Array(values) => Kind::ListValue(prost_types::ListValue {
            values: values
                .iter()
                .map(json_to_pb_value)
                .collect::<Result<Vec<_>, _>>()?,
        }),
        serde_json::Value::Object(_) => Kind::StructValue(json_to_struct(value)?),
    };
    Ok(PbValue { kind: Some(kind) })
}

pub(crate) fn write_envelope_from_pb(envelope: pb::WriteEnvelope) -> Result<WriteEnvelope, Status> {
    let payload = if envelope.payload.is_some() {
        struct_to_json(envelope.payload)?
    } else if !envelope.payload_bytes.is_empty() {
        serde_json::from_slice(&envelope.payload_bytes)
            .map_err(|_| Status::invalid_argument("payload_bytes is not valid JSON"))?
    } else {
        return Err(Status::invalid_argument("payload is required"));
    };
    if !payload.is_object() {
        return Err(Status::invalid_argument("payload must be a JSON object"));
    }
    if envelope.store.is_empty()
        || envelope.entity.is_empty()
        || envelope.key.is_empty()
        || envelope.op.is_empty()
        || envelope.idempotency_key.is_empty()
    {
        return Err(Status::invalid_argument(
            "op, store, entity, key and idempotency_key are required",
        ));
    }
    Ok(WriteEnvelope {
        op: envelope.op,
        store: envelope.store,
        entity: envelope.entity,
        key: envelope.key,
        version: envelope.version,
        ts: envelope.ts,
        idempotency_key: envelope.idempotency_key,
        ack_mode: envelope.ack_mode,
        payload,
    })
}

fn event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<pb::EventEnvelope> {
    let id: i64 = row.get(0)?;
    let payload: Vec<u8> = row.get(8)?;
    let payload_json: serde_json::Value = serde_json::from_slice(&payload).unwrap_or_default();
    let event_payload = payload_json
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    Ok(pb::EventEnvelope {
        event_id: row.get(1)?,
        event_type: row.get(2)?,
        store: row.get(3)?,
        entity: row.get(4)?,
        key: row.get(5)?,
        version: row.get::<_, i64>(6)? as u64,
        occurred_at: row.get(7)?,
        outbox_seq: id as u64,
        payload: Some(json_to_struct(&event_payload).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::other(error.to_string())),
            )
        })?),
        payload_bytes: Vec::new(),
    })
}

fn auth<T>(request: &Request<T>, config: &AuthConfig) -> Result<(), Status> {
    if !config.enabled {
        return Ok(());
    }
    let key = request
        .metadata()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("x-api-key metadata is required"))?;
    if crate::auth::session::validate_api_key(key, &config.valid_keys).is_some() {
        Ok(())
    } else {
        Err(Status::unauthenticated("invalid API key"))
    }
}

fn map_core_error(error: &ixmati_core::Error) -> Status {
    match error {
        ixmati_core::Error::QueueFull { .. } | ixmati_core::Error::OutboxBacklog { .. } => {
            let mut status = Status::resource_exhausted(error.to_string());
            let retry_after = std::env::var("THROTTLE_WINDOW_SECS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(1)
                .max(1)
                .to_string();
            if let Ok(value) = retry_after.parse() {
                status.metadata_mut().insert("retry-after", value);
            }
            status
        }
        ixmati_core::Error::StoreNotFound { .. } | ixmati_core::Error::EntityNotFound { .. } => {
            Status::not_found(error.to_string())
        }
        ixmati_core::Error::VersionConflict { .. } | ixmati_core::Error::Duplicate { .. } => {
            Status::already_exists(error.to_string())
        }
        ixmati_core::Error::WriteRejected { .. } => Status::invalid_argument(error.to_string()),
        ixmati_core::Error::Internal { .. } | ixmati_core::Error::ProjectionError { .. } => {
            Status::internal(error.to_string())
        }
    }
}

#[derive(Clone)]
pub struct WriteServiceImpl {
    state: AppState,
    auth: AuthConfig,
}

#[tonic::async_trait]
impl pb::write_service_server::WriteService for WriteServiceImpl {
    async fn write(
        &self,
        request: Request<pb::WriteRequest>,
    ) -> Result<Response<pb::WriteResponse>, Status> {
        auth(&request, &self.auth)?;
        let envelope = request
            .into_inner()
            .envelope
            .ok_or_else(|| Status::invalid_argument("envelope is required"))?;
        let envelope = write_envelope_from_pb(envelope)?;
        let result = rest::submit_write(self.state.clone(), envelope).await;
        match result {
            Ok((_status, response)) => {
                let ack = response.0;
                Ok(Response::new(pb::WriteResponse {
                    status: match ack.status.as_str() {
                        "APPLIED" => "COMMITTED".into(),
                        status => status.into(),
                    },
                    store: ack.store,
                    idempotency_key: ack.idempotency_key,
                    message: ack.message.unwrap_or_default(),
                }))
            }
            Err((_, error)) => Err(map_core_error(&error.0)),
        }
    }

    async fn get_write_status(
        &self,
        request: Request<pb::GetWriteStatusRequest>,
    ) -> Result<Response<pb::GetWriteStatusResponse>, Status> {
        auth(&request, &self.auth)?;
        let request = request.into_inner();
        if request.store.is_empty() || request.idempotency_key.is_empty() {
            return Err(Status::invalid_argument(
                "store and idempotency_key are required",
            ));
        }
        let db_path = self
            .state
            .db_path_for(&request.store)
            .ok_or_else(|| Status::unavailable("store database is not configured"))?;
        let status =
            crate::status::StatusQuery::query(db_path, &request.store, &request.idempotency_key)
                .map_err(|error| Status::internal(error.to_string()))?;
        let response = match status {
            crate::status::WriteStatus::Pending {
                store,
                idempotency_key,
            } => pb::GetWriteStatusResponse {
                status: 6,
                store,
                idempotency_key,
                detail: "PENDING".into(),
            },
            crate::status::WriteStatus::Applied {
                store,
                idempotency_key,
                entity,
                key,
                version,
                applied_at,
            } => pb::GetWriteStatusResponse {
                status: 2,
                store,
                idempotency_key,
                detail: format!("APPLIED {entity}/{key} v{version} at {applied_at}"),
            },
        };
        Ok(Response::new(response))
    }
}

#[derive(Clone)]
pub struct ReadServiceImpl {
    state: AppState,
    auth: AuthConfig,
}

#[tonic::async_trait]
impl pb::read_service_server::ReadService for ReadServiceImpl {
    async fn read(
        &self,
        request: Request<pb::ReadRequest>,
    ) -> Result<Response<pb::ReadResponse>, Status> {
        auth(&request, &self.auth)?;
        let request = request.into_inner();
        let response = rest::read_handler(
            axum::extract::State(self.state.clone()),
            axum::extract::Query(rest::ReadQuery {
                store: (!request.store.is_empty()).then_some(request.store),
                entity: (!request.entity.is_empty()).then_some(request.entity),
                key: (!request.key.is_empty()).then_some(request.key),
                projection: (!request.projection.is_empty()).then_some(request.projection),
            }),
        )
        .await
        .map_err(|(_, error)| map_core_error(&error.0))?;
        let value = response.0;
        Ok(Response::new(read_response_from_json(value)?))
    }
}

#[derive(Clone)]
pub struct HealthServiceImpl {
    state: AppState,
}

#[tonic::async_trait]
impl pb::health_service_server::HealthService for HealthServiceImpl {
    async fn check(
        &self,
        _request: Request<pb::HealthCheckRequest>,
    ) -> Result<Response<pb::HealthCheckResponse>, Status> {
        let mut checker = crate::health::HealthChecker::new().with_mqtt(self.state.broker());
        if let Some(path) = self.state.db_path_for("default") {
            checker = checker.with_db(path);
        }
        for (store, path) in self.state.configured_db_paths() {
            checker = checker.with_store_db(store, path);
        }
        let status = checker.check();
        let overall = match status.overall {
            crate::health::Health::Ok => 1,
            crate::health::Health::Degraded => 2,
            crate::health::Health::Unavailable => 3,
        };
        Ok(Response::new(pb::HealthCheckResponse {
            overall,
            components: status
                .components
                .into_iter()
                .map(|component| pb::ComponentHealth {
                    name: component.name,
                    status: match component.status {
                        crate::health::Health::Ok => 1,
                        crate::health::Health::Degraded => 2,
                        crate::health::Health::Unavailable => 3,
                    },
                    detail: component.detail.unwrap_or_default(),
                })
                .collect(),
        }))
    }
}

#[derive(Clone)]
pub struct EventServiceImpl {
    state: AppState,
    auth: AuthConfig,
}

type EventStream = Pin<Box<dyn Stream<Item = Result<pb::EventEnvelope, Status>> + Send + 'static>>;

#[tonic::async_trait]
impl pb::event_service_server::EventService for EventServiceImpl {
    type SubscribeEventsStream = EventStream;

    async fn subscribe_events(
        &self,
        request: Request<pb::SubscribeEventsRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        auth(&request, &self.auth)?;
        let request = request.into_inner();
        if request.store.is_empty() {
            return Err(Status::invalid_argument("store is required"));
        }
        if request.after_outbox_seq < 0 {
            return Err(Status::invalid_argument(
                "after_outbox_seq cannot be negative",
            ));
        }
        let db_path = self
            .state
            .db_path_for(&request.store)
            .ok_or_else(|| Status::not_found("store database is not configured"))?
            .to_owned();
        if request.after_outbox_seq > 0 {
            let db_path_for_check = db_path.clone();
            let store_for_check = request.store.clone();
            let after = request.after_outbox_seq;
            let cursor_error = tokio::task::spawn_blocking(move || {
                oldest_available_cursor(&db_path_for_check, &store_for_check, after)
            })
            .await
            .map_err(|error| Status::internal(error.to_string()))?
            .map_err(|error| Status::internal(error.to_string()))?;
            if let Some(message) = cursor_error {
                return Err(Status::out_of_range(message));
            }
        }
        let (tx, rx) = tokio::sync::mpsc::channel(event_stream_buffer_capacity_from_env());
        tokio::spawn(async move {
            let mut cursor = request.after_outbox_seq.max(0);
            let mut seen = HashSet::new();
            let filters = request;
            loop {
                let result = tokio::task::spawn_blocking({
                    let db_path = db_path.clone();
                    let store = filters.store.clone();
                    let entity = filters.entity.clone();
                    let key = filters.key.clone();
                    move || fetch_events(&db_path, &store, &entity, &key, cursor, 128)
                })
                .await;
                let rows = match result {
                    Ok(Ok(rows)) => rows,
                    Ok(Err(error)) => {
                        let _ = tx.send(Err(Status::internal(error.to_string()))).await;
                        break;
                    }
                    Err(error) => {
                        let _ = tx.send(Err(Status::internal(error.to_string()))).await;
                        break;
                    }
                };
                if rows.is_empty() {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                for (id, event) in rows {
                    let is_new = seen.insert(event.event_id.clone());
                    if seen.len() > 4096 {
                        seen.clear();
                    }
                    if !is_new {
                        cursor = id;
                        continue;
                    }
                    // Keep one bounded-channel slot free for the terminal
                    // RESOURCE_EXHAUSTED item.  Without this reservation a
                    // slow client filled the channel and the diagnostic error
                    // itself was dropped, making the stream look like a
                    // successful EOF and losing the resume cursor.
                    if tx.capacity() <= 1 {
                        let _ = tx.try_send(Err(Status::resource_exhausted(format!(
                            "event stream consumer is behind; resume from cursor {cursor}"
                        ))));
                        return;
                    }
                    let permit = match tx.try_reserve() {
                        Ok(permit) => permit,
                        Err(_) => return,
                    };
                    permit.send(Ok(event));
                    cursor = id;
                }
            }
        });
        Ok(Response::new(
            Box::pin(ReceiverStream::new(rx)) as Self::SubscribeEventsStream
        ))
    }
}

fn oldest_available_cursor(
    db_path: &str,
    store: &str,
    requested_cursor: i64,
) -> rusqlite::Result<Option<String>> {
    let conn = rusqlite::Connection::open(db_path)?;
    let oldest: Option<i64> = conn.query_row(
        "SELECT MIN(id) FROM _outbox WHERE store = ?1",
        rusqlite::params![store],
        |row| row.get(0),
    )?;
    Ok(oldest
        .filter(|oldest| requested_cursor < oldest.saturating_sub(1))
        .map(|oldest| {
            format!(
                "outbox cursor {requested_cursor} is older than retained history starting at {oldest}"
            )
        }))
}

fn fetch_events(
    db_path: &str,
    store: &str,
    entity: &str,
    key: &str,
    cursor: i64,
    limit: usize,
) -> rusqlite::Result<Vec<(i64, pb::EventEnvelope)>> {
    let conn = rusqlite::Connection::open(db_path)?;
    let sql = "SELECT id, event_id, event_type, store, entity, key, version, occurred_at, payload
               FROM _outbox
               WHERE store = ?1 AND id > ?2
                 AND (?3 = '' OR entity = ?3)
                 AND (?4 = '' OR key = ?4)
               ORDER BY id ASC LIMIT ?5";
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(rusqlite::params![store, cursor, entity, key, limit as i64])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        result.push((id, event_from_row(row)?));
    }
    Ok(result)
}

pub async fn serve(
    state: AppState,
    auth: AuthConfig,
    host: &str,
    port: u16,
) -> Result<(), tonic::transport::Error> {
    let addr = format!("{host}:{port}")
        .parse()
        .expect("valid gRPC address");
    let max_message_bytes = grpc_max_message_bytes_from_env();
    let max_concurrent_streams = grpc_max_concurrent_streams_from_env();
    tracing::info!(
        max_message_bytes,
        max_concurrent_streams,
        "gRPC resource limits configured"
    );

    Server::builder()
        .max_concurrent_streams(Some(max_concurrent_streams))
        .add_service(
            pb::write_service_server::WriteServiceServer::new(WriteServiceImpl {
                state: state.clone(),
                auth: auth.clone(),
            })
            .max_decoding_message_size(max_message_bytes)
            .max_encoding_message_size(max_message_bytes),
        )
        .add_service(
            pb::read_service_server::ReadServiceServer::new(ReadServiceImpl {
                state: state.clone(),
                auth: auth.clone(),
            })
            .max_decoding_message_size(max_message_bytes)
            .max_encoding_message_size(max_message_bytes),
        )
        .add_service(
            pb::health_service_server::HealthServiceServer::new(HealthServiceImpl {
                state: state.clone(),
            })
            .max_decoding_message_size(max_message_bytes)
            .max_encoding_message_size(max_message_bytes),
        )
        .add_service(
            pb::event_service_server::EventServiceServer::new(EventServiceImpl { state, auth })
                .max_decoding_message_size(max_message_bytes)
                .max_encoding_message_size(max_message_bytes),
        )
        .serve(addr)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pb::event_service_server::EventService;
    use tokio_stream::StreamExt;

    #[test]
    fn struct_round_trip_preserves_nested_json() {
        let value = serde_json::json!({
            "customer": {"id": "c-1", "active": true},
            "items": [1.25, "two", null],
            "total": 12.5
        });
        let encoded = json_to_struct(&value).expect("object converts to Struct");
        let decoded = struct_to_json(Some(encoded)).expect("Struct converts to object");
        assert_eq!(decoded, value);
    }

    #[test]
    fn struct_conversion_rejects_non_object_payload() {
        let error = json_to_struct(&serde_json::json!("not-an-object")).unwrap_err();
        assert_eq!(error.code(), tonic::Code::Internal);
    }

    #[test]
    fn grpc_auth_uses_x_api_key_metadata() {
        let config = AuthConfig::new(vec![crate::auth::ApiKey {
            key_id: "test-key".into(),
            store_access: Vec::new(),
            created_at: chrono::Utc::now(),
        }]);
        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert("x-api-key", "test-key".parse().unwrap());
        auth(&request, &config).expect("valid metadata key");

        request
            .metadata_mut()
            .insert("x-api-key", "wrong-key".parse().unwrap());
        assert_eq!(
            auth(&request, &config).unwrap_err().code(),
            tonic::Code::Unauthenticated
        );
    }

    #[test]
    fn protobuf_write_accepts_deprecated_json_bytes() {
        let envelope = write_envelope_from_pb(pb::WriteEnvelope {
            op: "upsert".into(),
            store: "orders".into(),
            entity: "order".into(),
            key: "o-1".into(),
            version: 4,
            ts: "2026-08-12T00:00:00Z".into(),
            idempotency_key: "idem-1".into(),
            ack_mode: "committed".into(),
            payload: None,
            payload_bytes: br#"{"status":"paid"}"#.to_vec(),
        })
        .expect("deprecated bytes remain compatible");
        assert_eq!(envelope.payload, serde_json::json!({"status": "paid"}));
    }

    #[test]
    fn protobuf_write_requires_object_payload() {
        let error = write_envelope_from_pb(pb::WriteEnvelope {
            op: "upsert".into(),
            store: "orders".into(),
            entity: "order".into(),
            key: "o-1".into(),
            idempotency_key: "idem-empty-object".into(),
            payload: Some(Struct::default()),
            ..Default::default()
        })
        .expect("empty Struct is an object");
        assert_eq!(error.payload, serde_json::json!({}));

        let error = write_envelope_from_pb(pb::WriteEnvelope {
            op: "upsert".into(),
            store: "orders".into(),
            entity: "order".into(),
            key: "o-1".into(),
            ..Default::default()
        })
        .unwrap_err();
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn old_cursor_is_reported_when_outbox_history_was_retained_after_cleanup() {
        let path =
            std::env::temp_dir().join(format!("ixmati-grpc-cursor-{}.db", uuid::Uuid::new_v4()));
        let conn = rusqlite::Connection::open(&path).expect("temporary SQLite database");
        conn.execute_batch(
            "CREATE TABLE _outbox (id INTEGER PRIMARY KEY, store TEXT NOT NULL);
             INSERT INTO _outbox (id, store) VALUES (10, 'orders');",
        )
        .expect("outbox fixture");
        drop(conn);

        let error = oldest_available_cursor(path.to_str().unwrap(), "orders", 1)
            .expect("cursor query")
            .expect("cursor must be out of range");
        assert!(error.contains("starting at 10"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn event_replay_uses_outbox_id_as_cursor_and_applies_filters() {
        let path =
            std::env::temp_dir().join(format!("ixmati-grpc-events-{}.db", uuid::Uuid::new_v4()));
        let conn = rusqlite::Connection::open(&path).expect("temporary SQLite database");
        conn.execute_batch(
            "CREATE TABLE _outbox (
                id INTEGER PRIMARY KEY,
                event_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                store TEXT NOT NULL,
                entity TEXT NOT NULL,
                key TEXT NOT NULL,
                version INTEGER NOT NULL,
                occurred_at TEXT NOT NULL,
                payload BLOB NOT NULL,
                published_at TEXT
            );",
        )
        .expect("outbox schema");
        let payload = serde_json::to_vec(&serde_json::json!({
            "event_id": "evt-1",
            "payload": {"status": "paid"}
        }))
        .unwrap();
        conn.execute(
            "INSERT INTO _outbox (id,event_id,event_type,store,entity,key,version,occurred_at,payload)
             VALUES (7,'evt-1','order.updated','orders','order','o-1',2,'2026-08-12T00:00:00Z',?1)",
            rusqlite::params![payload],
        )
        .expect("event fixture");
        drop(conn);

        let rows = fetch_events(path.to_str().unwrap(), "orders", "order", "o-1", 0, 10)
            .expect("event replay query");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, 7);
        assert_eq!(rows[0].1.outbox_seq, 7);
        let event_payload = rows[0].1.payload.as_ref().unwrap();
        assert!(matches!(
            event_payload.fields["status"].kind.as_ref(),
            Some(Kind::StringValue(value)) if value == "paid"
        ));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn slow_event_client_receives_resource_exhausted_with_cursor() {
        let path = std::env::temp_dir().join(format!(
            "ixmati-grpc-slow-stream-{}.db",
            uuid::Uuid::new_v4()
        ));
        let conn = rusqlite::Connection::open(&path).expect("temporary SQLite database");
        conn.execute_batch(
            "CREATE TABLE _outbox (
                id INTEGER PRIMARY KEY,
                event_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                store TEXT NOT NULL,
                entity TEXT NOT NULL,
                key TEXT NOT NULL,
                version INTEGER NOT NULL,
                occurred_at TEXT NOT NULL,
                payload BLOB NOT NULL,
                published_at TEXT
            );",
        )
        .expect("outbox schema");
        let payload = serde_json::to_vec(&serde_json::json!({
            "event_id": "evt",
            "payload": {"status": "paid"}
        }))
        .unwrap();
        for id in 1..=130 {
            conn.execute(
                "INSERT INTO _outbox (id,event_id,event_type,store,entity,key,version,occurred_at,payload)
                 VALUES (?1,?2,'order.updated','orders','order',?3,1,'2026-08-12T00:00:00Z',?4)",
                rusqlite::params![id, format!("evt-{id}"), format!("o-{id}"), &payload],
            )
            .expect("event fixture");
        }
        drop(conn);

        let state = crate::rest::AppState::new(
            "tcp://127.0.0.1:1",
            std::sync::Arc::new(ixmati_cache::NoOpBackend::new()),
            None,
            None,
            None,
        )
        .with_db_paths(std::collections::HashMap::from([(
            "orders".to_string(),
            path.to_string_lossy().into_owned(),
        )]));
        let service = EventServiceImpl {
            state,
            auth: AuthConfig::disabled(),
        };
        let response = service
            .subscribe_events(Request::new(pb::SubscribeEventsRequest {
                store: "orders".into(),
                ..Default::default()
            }))
            .await
            .expect("subscribe");
        let mut stream = response.into_inner();

        // Do not consume while the producer fills the bounded queue.  The
        // implementation must still leave a terminal diagnostic available.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut saw_resource_exhausted = false;
        while let Some(item) = stream.next().await {
            match item {
                Ok(_) => {}
                Err(error) => {
                    assert_eq!(error.code(), tonic::Code::ResourceExhausted);
                    assert!(error.message().contains("resume from cursor"));
                    saw_resource_exhausted = true;
                    break;
                }
            }
        }
        assert!(
            saw_resource_exhausted,
            "slow client must receive a terminal status"
        );

        let _ = std::fs::remove_file(path);
    }
}

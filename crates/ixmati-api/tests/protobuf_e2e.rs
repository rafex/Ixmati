//! E2E client for a running API + writer + MQTT stack.
//!
//! Run with:
//! `IXMATI_E2E_GRPC=http://127.0.0.1:30312 cargo test -p ixmati-api --test protobuf_e2e -- --nocapture`

#![allow(deprecated)]

use ixmati_api::grpc::pb;
use prost_types::{Struct, Value, value::Kind};
use std::collections::BTreeMap;
use tonic::{Request, metadata::MetadataValue};

fn grpc_endpoint() -> Option<String> {
    std::env::var("IXMATI_E2E_GRPC")
        .ok()
        .filter(|value| !value.is_empty())
}

fn authenticated<T>(message: T) -> Request<T> {
    authenticated_with_key(
        message,
        &std::env::var("IXMATI_E2E_API_KEY").unwrap_or_default(),
    )
}

fn authenticated_with_key<T>(message: T, key: &str) -> Request<T> {
    let mut request = Request::new(message);
    if !key.is_empty() {
        request.metadata_mut().insert(
            "x-api-key",
            MetadataValue::try_from(key).expect("API key must be valid metadata"),
        );
    }
    request
}

fn payload(order_id: &str) -> Struct {
    Struct {
        fields: BTreeMap::from([
            (
                "order_id".into(),
                Value {
                    kind: Some(Kind::StringValue(order_id.into())),
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

#[tokio::test]
async fn grpc_unary_and_event_stream_round_trip() {
    let Some(endpoint) = grpc_endpoint() else {
        eprintln!("skipped: set IXMATI_E2E_GRPC to run against a live stack");
        return;
    };
    let api_key = std::env::var("IXMATI_E2E_API_KEY").unwrap_or_default();
    assert!(
        !api_key.is_empty(),
        "IXMATI_E2E_API_KEY is required for E2E"
    );

    let mut health = pb::health_service_client::HealthServiceClient::connect(endpoint.clone())
        .await
        .expect("connect HealthService");
    let health_response = health
        .check(authenticated(pb::HealthCheckRequest {}))
        .await
        .expect("HealthService.Check")
        .into_inner();
    assert_ne!(health_response.overall, 0);

    let mut unauthenticated_writes =
        pb::write_service_client::WriteServiceClient::connect(endpoint.clone())
            .await
            .expect("connect unauthenticated WriteService");
    let unauthenticated_error = unauthenticated_writes
        .write(Request::new(pb::WriteRequest { envelope: None }))
        .await
        .expect_err("missing x-api-key must be rejected");
    assert_eq!(unauthenticated_error.code(), tonic::Code::Unauthenticated);

    let mut invalid_key_writes =
        pb::write_service_client::WriteServiceClient::connect(endpoint.clone())
            .await
            .expect("connect invalid-key WriteService");
    let invalid_key_error = invalid_key_writes
        .write(authenticated_with_key(
            pb::WriteRequest { envelope: None },
            "invalid-key",
        ))
        .await
        .expect_err("invalid x-api-key must be rejected");
    assert_eq!(invalid_key_error.code(), tonic::Code::Unauthenticated);

    let mut argument_events =
        pb::event_service_client::EventServiceClient::connect(endpoint.clone())
            .await
            .expect("connect argument EventService");
    let missing_store_error = argument_events
        .subscribe_events(authenticated(pb::SubscribeEventsRequest::default()))
        .await
        .expect_err("empty store must be rejected");
    assert_eq!(missing_store_error.code(), tonic::Code::InvalidArgument);

    let negative_cursor_error = argument_events
        .subscribe_events(authenticated(pb::SubscribeEventsRequest {
            store: "smoke".into(),
            after_outbox_seq: -1,
            ..Default::default()
        }))
        .await
        .expect_err("negative cursor must be rejected");
    assert_eq!(negative_cursor_error.code(), tonic::Code::InvalidArgument);

    let mut argument_status =
        pb::write_service_client::WriteServiceClient::connect(endpoint.clone())
            .await
            .expect("connect argument status service");
    let missing_status_error = argument_status
        .get_write_status(authenticated(pb::GetWriteStatusRequest::default()))
        .await
        .expect_err("empty status request must be rejected");
    assert_eq!(missing_status_error.code(), tonic::Code::InvalidArgument);

    let idempotency_key = format!("protobuf-e2e-{}", uuid::Uuid::new_v4());
    let order_key = format!("order-{}", uuid::Uuid::new_v4());

    let mut events = pb::event_service_client::EventServiceClient::connect(endpoint.clone())
        .await
        .expect("connect EventService");
    let mut event_stream = events
        .subscribe_events(authenticated(pb::SubscribeEventsRequest {
            store: "smoke".into(),
            entity: "order".into(),
            key: order_key.clone(),
            after_outbox_seq: 0,
        }))
        .await
        .expect("EventService.SubscribeEvents")
        .into_inner();

    let mut writes = pb::write_service_client::WriteServiceClient::connect(endpoint.clone())
        .await
        .expect("connect WriteService");
    let write_response = writes
        .write(authenticated(pb::WriteRequest {
            envelope: Some(pb::WriteEnvelope {
                op: "upsert".into(),
                store: "smoke".into(),
                entity: "order".into(),
                key: order_key.clone(),
                version: 1,
                ts: "2026-08-12T00:00:00Z".into(),
                idempotency_key: idempotency_key.clone(),
                ack_mode: "committed".into(),
                payload: Some(payload(&order_key)),
                payload_bytes: Vec::new(),
            }),
        }))
        .await
        .expect("WriteService.Write")
        .into_inner();
    assert_eq!(write_response.status, "COMMITTED");
    assert_eq!(write_response.idempotency_key, idempotency_key);

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_stream.message())
        .await
        .expect("event stream timeout")
        .expect("event stream transport")
        .expect("event stream ended");
    assert_eq!(event.entity, "order");
    assert_eq!(event.key, order_key);
    assert!(event.outbox_seq > 0);
    assert_eq!(
        event
            .payload
            .expect("event payload")
            .fields
            .get("order_id")
            .and_then(|value| value.kind.as_ref()),
        Some(&Kind::StringValue(event.key.clone()))
    );

    let status = writes
        .get_write_status(authenticated(pb::GetWriteStatusRequest {
            store: "smoke".into(),
            idempotency_key,
        }))
        .await
        .expect("WriteService.GetWriteStatus")
        .into_inner();
    assert_eq!(status.status, pb::WriteStatus::Committed as i32);

    let mut reads = pb::read_service_client::ReadServiceClient::connect(endpoint)
        .await
        .expect("connect ReadService");
    let read = reads
        .read(authenticated(pb::ReadRequest {
            store: "smoke".into(),
            entity: "order".into(),
            key: event.key,
            projection: String::new(),
        }))
        .await
        .expect("ReadService.Read")
        .into_inner();
    assert!(read.found);
    assert!(read.payload.is_some());
}

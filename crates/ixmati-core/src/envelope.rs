use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteEnvelope {
    pub op: String,
    pub store: String,
    pub entity: String,
    pub key: String,
    pub version: u64,
    pub ts: String,
    pub idempotency_key: String,
    pub ack_mode: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: String,
    pub event_type: String,
    pub store: String,
    pub entity: String,
    pub key: String,
    pub version: u64,
    pub occurred_at: String,
    pub outbox_seq: u64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckResponse {
    pub status: String,
    pub store: String,
    pub idempotency_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

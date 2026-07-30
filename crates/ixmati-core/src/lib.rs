pub mod config;
pub mod envelope;
pub mod error;
pub mod projection;
pub mod store;

pub use config::{Config, StoreConfig};
pub use envelope::{AckResponse, EventEnvelope, WriteEnvelope};
pub use error::{Error, Result};
pub use projection::{CopyField, ProjectionConfig, ProjectionPattern, ProjectionRegistry};
pub use store::{Store, StoreRegistry, Topology};

#[cfg(test)]
mod tests {
    use super::*;
    use envelope::*;

    #[test]
    fn write_envelope_serialization() {
        let envelope = WriteEnvelope {
            op: "upsert".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_abc123".into(),
            version: 7,
            ts: "2026-07-29T10:30:00Z".into(),
            idempotency_key: "550e8400-e29b-41d4-a716-446655440000".into(),
            ack_mode: "committed".into(),
            payload: serde_json::json!({"total": 1500.0}),
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: WriteEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.op, "upsert");
        assert_eq!(decoded.store, "pedidos");
        assert_eq!(decoded.entity, "pedido");
        assert_eq!(decoded.key, "ped_abc123");
        assert_eq!(decoded.version, 7);
        assert_eq!(decoded.ack_mode, "committed");
    }

    #[test]
    fn event_envelope_serialization() {
        let envelope = EventEnvelope {
            event_id: "660e8400-e29b-41d4-a716-446655440001".into(),
            event_type: "pedido.creado".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: "ped_abc123".into(),
            version: 7,
            occurred_at: "2026-07-29T10:30:00.123Z".into(),
            outbox_seq: 1042,
            payload: serde_json::json!({"total": 1500.0}),
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: EventEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.event_type, "pedido.creado");
        assert_eq!(decoded.version, 7);
        assert_eq!(decoded.outbox_seq, 1042);
        assert_eq!(decoded.store, "pedidos");
    }

    #[test]
    fn ack_response_serialization() {
        let ack = AckResponse {
            status: "COMMITTED".into(),
            store: "pedidos".into(),
            idempotency_key: "550e8400-e29b-41d4-a716-446655440000".into(),
            message: Some("ok".into()),
        };

        let json = serde_json::to_string(&ack).unwrap();
        assert!(json.contains("COMMITTED"));
    }
}

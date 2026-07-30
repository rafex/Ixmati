use chrono::Utc;
use ixmati_core::{AckResponse, EventEnvelope, WriteEnvelope};
use rusqlite::Connection;
use uuid::Uuid;

use crate::dedup::{IdempotencyStatus, IdempotencyTracker};
use crate::outbox::Outbox;
use crate::BatchResult;

pub struct WriteEngine;

impl WriteEngine {
    pub fn ensure_schema(conn: &Connection, store_name: &str) -> rusqlite::Result<()> {
        IdempotencyTracker::ensure_schema(conn)?;
        Outbox::ensure_schema(conn)?;

        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS payload_{store_name} (
                entity  TEXT NOT NULL,
                key     TEXT NOT NULL,
                version INTEGER NOT NULL,
                payload BLOB NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (entity, key)
            );"
        ))?;

        Ok(())
    }

    pub fn process_batch(
        conn: &mut Connection,
        commands: &[WriteEnvelope],
    ) -> rusqlite::Result<BatchResult> {
        let store = commands
            .first()
            .map(|c| c.store.clone())
            .unwrap_or_default();

        let mut result = BatchResult {
            store,
            committed: 0,
            duplicates: 0,
            version_conflicts: 0,
            events: Vec::new(),
        };

        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

        for cmd in commands {
            let status = IdempotencyTracker::check(&tx, cmd)?;

            match status {
                IdempotencyStatus::AlreadyApplied { .. } => {
                    result.duplicates += 1;
                    continue;
                }
                IdempotencyStatus::New => {}
            }

            let current_version =
                IdempotencyTracker::current_version(&tx, &cmd.store, &cmd.entity, &cmd.key)?;

            if let Some(stored) = current_version {
                if cmd.version <= stored {
                    result.version_conflicts += 1;
                    continue;
                }
            }

            match cmd.op.as_str() {
                "upsert" | "patch" => {
                    let payload = serde_json::to_vec(&cmd.payload).unwrap_or_default();
                    tx.execute(
                        &format!(
                            "INSERT INTO payload_{store} (entity, key, version, payload, updated_at)
                             VALUES (?1, ?2, ?3, ?4, datetime('now'))
                             ON CONFLICT(entity, key) DO UPDATE SET
                                version    = excluded.version,
                                payload    = excluded.payload,
                                updated_at = excluded.updated_at",
                            store = cmd.store
                        ),
                        rusqlite::params![cmd.entity, cmd.key, cmd.version as i64, payload],
                    )?;
                }
                "delete" => {
                    tx.execute(
                        &format!(
                            "DELETE FROM payload_{store} WHERE entity = ?1 AND key = ?2",
                            store = cmd.store
                        ),
                        rusqlite::params![cmd.entity, cmd.key],
                    )?;
                }
                _ => {
                    tracing::warn!(op = %cmd.op, "unknown operation");
                    continue;
                }
            }

            let event = EventEnvelope {
                event_id: Uuid::new_v4().to_string(),
                event_type: format!("{}.{}", cmd.entity, event_suffix(&cmd.op)),
                store: cmd.store.clone(),
                entity: cmd.entity.clone(),
                key: cmd.key.clone(),
                version: cmd.version,
                occurred_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                outbox_seq: 0,
                payload: cmd.payload.clone(),
            };

            Outbox::insert(&tx, &event)?;
            IdempotencyTracker::record(&tx, cmd)?;

            result.committed += 1;
            result.events.push(event);
        }

        tx.commit()?;
        Ok(result)
    }

    pub fn format_ack(
        envelope: &WriteEnvelope,
        ack_mode: &str,
        message: Option<String>,
    ) -> AckResponse {
        AckResponse {
            status: match ack_mode {
                "committed" => "COMMITTED".into(),
                _ => "ACCEPTED".into(),
            },
            store: envelope.store.clone(),
            idempotency_key: envelope.idempotency_key.clone(),
            message,
        }
    }
}

fn event_suffix(op: &str) -> &str {
    match op {
        "upsert" => "creado",
        "delete" => "eliminado",
        "patch" => "actualizado",
        _ => "modificado",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_cmd(op: &str, key: &str, version: u64, idem: &str) -> WriteEnvelope {
        WriteEnvelope {
            op: op.into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: key.into(),
            version,
            ts: Utc::now().to_rfc3339(),
            idempotency_key: idem.into(),
            ack_mode: "committed".into(),
            payload: json!({"total": 100.0, "estado": "pendiente"}),
        }
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        WriteEngine::ensure_schema(&conn, "pedidos").unwrap();
        conn
    }

    #[test]
    fn upsert_new_entity() {
        let mut conn = setup_db();
        let cmds = vec![make_cmd("upsert", "ped_1", 1, "ik-1")];

        let result = WriteEngine::process_batch(&mut conn, &cmds).unwrap();
        assert_eq!(result.committed, 1);
        assert_eq!(result.duplicates, 0);
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_type, "pedido.creado");
    }

    #[test]
    fn duplicate_idempotency_key_is_skipped() {
        let mut conn = setup_db();

        let cmds = vec![make_cmd("upsert", "ped_1", 1, "ik-1")];
        let result = WriteEngine::process_batch(&mut conn, &cmds).unwrap();
        assert_eq!(result.committed, 1);

        let result2 = WriteEngine::process_batch(&mut conn, &cmds).unwrap();
        assert_eq!(result2.committed, 0);
        assert_eq!(result2.duplicates, 1);
        assert!(result2.events.is_empty());
    }

    #[test]
    fn version_conflict_rejected() {
        let mut conn = setup_db();

        WriteEngine::process_batch(&mut conn, &[make_cmd("upsert", "ped_1", 5, "ik-1")])
            .unwrap();

        let result = WriteEngine::process_batch(
            &mut conn,
            &[make_cmd("upsert", "ped_1", 3, "ik-2")],
        )
        .unwrap();
        assert_eq!(result.version_conflicts, 1);
        assert_eq!(result.committed, 0);
    }

    #[test]
    fn higher_version_accepted() {
        let mut conn = setup_db();

        WriteEngine::process_batch(&mut conn, &[make_cmd("upsert", "ped_1", 1, "ik-1")])
            .unwrap();

        let result = WriteEngine::process_batch(
            &mut conn,
            &[make_cmd("upsert", "ped_1", 2, "ik-2")],
        )
        .unwrap();
        assert_eq!(result.committed, 1);
        assert_eq!(result.version_conflicts, 0);
    }

    #[test]
    fn delete_removes_entity() {
        let mut conn = setup_db();

        WriteEngine::process_batch(&mut conn, &[make_cmd("upsert", "ped_1", 1, "ik-1")])
            .unwrap();

        let result = WriteEngine::process_batch(
            &mut conn,
            &[make_cmd("delete", "ped_1", 2, "ik-2")],
        )
        .unwrap();
        assert_eq!(result.committed, 1);
        assert_eq!(result.events[0].event_type, "pedido.eliminado");
    }

    #[test]
    fn events_in_outbox_are_fetchable() {
        let mut conn = setup_db();

        WriteEngine::process_batch(&mut conn, &[make_cmd("upsert", "ped_1", 1, "ik-1")])
            .unwrap();

        let rows = Outbox::fetch_unpublished(&conn, "pedidos", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event_type, "pedido.creado");
    }

    #[test]
    fn batch_with_mixed_results() {
        let mut conn = setup_db();

        let cmds = vec![
            make_cmd("upsert", "ped_1", 1, "ik-1"), // committed
            make_cmd("upsert", "ped_1", 1, "ik-1"), // duplicate
            make_cmd("upsert", "ped_2", 1, "ik-2"), // committed
            make_cmd("upsert", "ped_2", 0, "ik-3"), // version conflict
        ];

        let result = WriteEngine::process_batch(&mut conn, &cmds).unwrap();
        assert_eq!(result.committed, 2);
        assert_eq!(result.duplicates, 1);
        assert_eq!(result.version_conflicts, 1);
        assert_eq!(result.events.len(), 2);
    }
}

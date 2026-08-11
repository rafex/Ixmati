use ixmati_core::WriteEnvelope;
use rusqlite::Connection;

pub struct IdempotencyTracker;

impl IdempotencyTracker {
    pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _idempotency (
                idempotency_key TEXT NOT NULL,
                store           TEXT NOT NULL,
                entity          TEXT NOT NULL,
                key             TEXT NOT NULL,
                version         INTEGER NOT NULL,
                applied_at      TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (store, idempotency_key)
            );

            CREATE INDEX IF NOT EXISTS idx_idempotency_entity_key_version
                ON _idempotency(store, entity, key, version);",
        )
    }

    pub fn check(
        conn: &Connection,
        envelope: &WriteEnvelope,
    ) -> rusqlite::Result<IdempotencyStatus> {
        let mut stmt = conn.prepare(
            "SELECT version FROM _idempotency WHERE store = ?1 AND idempotency_key = ?2",
        )?;

        let mut rows = stmt.query((&envelope.store, &envelope.idempotency_key))?;

        match rows.next()? {
            Some(row) => {
                let stored_version: i64 = row.get(0)?;
                Ok(IdempotencyStatus::AlreadyApplied {
                    stored_version: stored_version as u64,
                })
            }
            None => Ok(IdempotencyStatus::New),
        }
    }

    pub fn record(conn: &Connection, envelope: &WriteEnvelope) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT OR IGNORE INTO _idempotency (idempotency_key, store, entity, key, version)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &envelope.idempotency_key,
                &envelope.store,
                &envelope.entity,
                &envelope.key,
                envelope.version as i64,
            ),
        )?;
        Ok(())
    }

    pub fn current_version(
        conn: &Connection,
        store: &str,
        entity: &str,
        key: &str,
    ) -> rusqlite::Result<Option<u64>> {
        let mut stmt = conn.prepare(
            "SELECT MAX(version) FROM _idempotency WHERE store = ?1 AND entity = ?2 AND key = ?3",
        )?;
        let mut rows = stmt.query((store, entity, key))?;

        match rows.next()? {
            Some(row) => {
                let v: Option<i64> = row.get(0)?;
                Ok(v.map(|v| v as u64))
            }
            None => Ok(None),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum IdempotencyStatus {
    New,
    AlreadyApplied { stored_version: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cmd(key: &str, version: u64, idempotency_key: &str) -> WriteEnvelope {
        WriteEnvelope {
            op: "upsert".into(),
            store: "pedidos".into(),
            entity: "pedido".into(),
            key: key.into(),
            version,
            ts: "2026-07-30T00:00:00Z".into(),
            idempotency_key: idempotency_key.into(),
            ack_mode: "committed".into(),
            payload: serde_json::json!({"n": 1}),
        }
    }

    #[test]
    fn new_idempotency_key_is_new() {
        let conn = Connection::open_in_memory().unwrap();
        IdempotencyTracker::ensure_schema(&conn).unwrap();

        let cmd = make_cmd("ped_1", 1, "ik-1");
        let status = IdempotencyTracker::check(&conn, &cmd).unwrap();
        assert_eq!(status, IdempotencyStatus::New);
    }

    #[test]
    fn duplicate_idempotency_key_is_rejected() {
        let conn = Connection::open_in_memory().unwrap();
        IdempotencyTracker::ensure_schema(&conn).unwrap();

        let cmd = make_cmd("ped_1", 1, "ik-1");
        IdempotencyTracker::record(&conn, &cmd).unwrap();

        let status = IdempotencyTracker::check(&conn, &cmd).unwrap();
        assert_eq!(
            status,
            IdempotencyStatus::AlreadyApplied { stored_version: 1 }
        );
    }

    #[test]
    fn same_key_different_store_is_independent() {
        let conn = Connection::open_in_memory().unwrap();
        IdempotencyTracker::ensure_schema(&conn).unwrap();

        let cmd_pedidos = WriteEnvelope {
            store: "pedidos".into(),
            ..make_cmd("ped_1", 1, "ik-1")
        };
        IdempotencyTracker::record(&conn, &cmd_pedidos).unwrap();

        let cmd_usuarios = WriteEnvelope {
            store: "usuarios".into(),
            ..make_cmd("usr_1", 1, "ik-1")
        };
        let status = IdempotencyTracker::check(&conn, &cmd_usuarios).unwrap();
        assert_eq!(status, IdempotencyStatus::New);
    }

    #[test]
    fn current_version_tracks_max() {
        let conn = Connection::open_in_memory().unwrap();
        IdempotencyTracker::ensure_schema(&conn).unwrap();

        IdempotencyTracker::record(&conn, &make_cmd("ped_1", 1, "ik-1")).unwrap();
        IdempotencyTracker::record(&conn, &make_cmd("ped_1", 3, "ik-2")).unwrap();
        IdempotencyTracker::record(&conn, &make_cmd("ped_1", 2, "ik-3")).unwrap();

        let v = IdempotencyTracker::current_version(&conn, "pedidos", "pedido", "ped_1").unwrap();
        assert_eq!(v, Some(3));
    }

    #[test]
    fn current_version_uses_the_entity_key_index() {
        let conn = Connection::open_in_memory().unwrap();
        IdempotencyTracker::ensure_schema(&conn).unwrap();

        let detail: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN
                 SELECT MAX(version) FROM _idempotency
                 WHERE store = 'pedidos' AND entity = 'pedido' AND key = 'ped_1'",
                [],
                |row| row.get(3),
            )
            .unwrap();

        assert!(
            detail.contains("idx_idempotency_entity_key_version"),
            "current_version must use the composite index, got: {detail}"
        );
    }

    #[test]
    fn ensure_schema_migrates_an_existing_idempotency_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE _idempotency (
                store TEXT NOT NULL,
                idempotency_key TEXT NOT NULL,
                entity TEXT NOT NULL,
                key TEXT NOT NULL,
                version INTEGER NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (store, idempotency_key)
            );",
        )
        .unwrap();

        IdempotencyTracker::ensure_schema(&conn).unwrap();

        let index_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_idempotency_entity_key_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(index_sql.contains("ON _idempotency(store, entity, key, version)"));
    }
}

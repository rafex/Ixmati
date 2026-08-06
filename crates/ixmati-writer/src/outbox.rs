use ixmati_core::EventEnvelope;
use rusqlite::Connection;

pub struct Outbox;

impl Outbox {
    pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _outbox (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id      TEXT NOT NULL,
                event_type    TEXT NOT NULL,
                store         TEXT NOT NULL,
                entity        TEXT NOT NULL,
                key           TEXT NOT NULL,
                version       INTEGER NOT NULL,
                occurred_at   TEXT NOT NULL,
                payload       BLOB NOT NULL,
                published_at  TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_outbox_published
                ON _outbox(published_at);

            CREATE INDEX IF NOT EXISTS idx_outbox_store
                ON _outbox(store, published_at);",
        )
    }

    pub fn insert(conn: &Connection, event: &EventEnvelope) -> rusqlite::Result<i64> {
        let payload = serde_json::to_vec(event).unwrap_or_default();

        conn.execute(
            "INSERT INTO _outbox (event_id, event_type, store, entity, key, version, occurred_at, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                event.event_id,
                event.event_type,
                event.store,
                event.entity,
                event.key,
                event.version as i64,
                event.occurred_at,
                payload,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    pub fn mark_published(conn: &Connection, outbox_id: i64) -> rusqlite::Result<()> {
        conn.execute(
            "UPDATE _outbox SET published_at = datetime('now') WHERE id = ?1",
            [outbox_id],
        )?;
        Ok(())
    }

    pub fn fetch_unpublished(
        conn: &Connection,
        store: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<OutboxRow>> {
        let mut stmt = conn.prepare(
            "SELECT id, event_id, event_type, store, entity, key, version, occurred_at, payload
             FROM _outbox
             WHERE store = ?1 AND published_at IS NULL
             ORDER BY id ASC
             LIMIT ?2",
        )?;

        let rows = stmt
            .query_map((store, limit as i64), |row| {
                Ok(OutboxRow {
                    id: row.get(0)?,
                    event_id: row.get(1)?,
                    event_type: row.get(2)?,
                    store: row.get(3)?,
                    entity: row.get(4)?,
                    key: row.get(5)?,
                    version: {
                        let v: i64 = row.get(6)?;
                        v as u64
                    },
                    occurred_at: row.get(7)?,
                    payload: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(rows)
    }

    pub fn cleanup_old(conn: &Connection, older_than_days: i64) -> rusqlite::Result<usize> {
        let deleted = conn.execute(
            "DELETE FROM _outbox WHERE published_at IS NOT NULL
             AND published_at < datetime('now', ?1)",
            [format!("-{} days", older_than_days)],
        )?;
        Ok(deleted)
    }
}

#[derive(Debug)]
pub struct OutboxRow {
    pub id: i64,
    pub event_id: String,
    pub event_type: String,
    pub store: String,
    pub entity: String,
    pub key: String,
    pub version: u64,
    pub occurred_at: String,
    pub payload: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ixmati_core::EventEnvelope;

    fn make_event(id: &str, store: &str, version: u64) -> EventEnvelope {
        EventEnvelope {
            event_id: id.into(),
            event_type: "pedido.creado".into(),
            store: store.into(),
            entity: "pedido".into(),
            key: format!("ped_{}", id),
            version,
            occurred_at: "2026-07-30T00:00:00Z".into(),
            outbox_seq: 1,
            payload: serde_json::json!({"total": 100}),
        }
    }

    #[test]
    fn insert_and_fetch_unpublished() {
        let conn = Connection::open_in_memory().unwrap();
        Outbox::ensure_schema(&conn).unwrap();

        let id = Outbox::insert(&conn, &make_event("evt-1", "pedidos", 1)).unwrap();
        assert!(id > 0);

        let rows = Outbox::fetch_unpublished(&conn, "pedidos", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event_id, "evt-1");
        assert_eq!(rows[0].store, "pedidos");
    }

    #[test]
    fn mark_published_removes_from_unpublished() {
        let conn = Connection::open_in_memory().unwrap();
        Outbox::ensure_schema(&conn).unwrap();

        let id = Outbox::insert(&conn, &make_event("evt-1", "pedidos", 1)).unwrap();
        Outbox::mark_published(&conn, id).unwrap();

        let rows = Outbox::fetch_unpublished(&conn, "pedidos", 10).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn unpolished_on_other_store() {
        let conn = Connection::open_in_memory().unwrap();
        Outbox::ensure_schema(&conn).unwrap();

        Outbox::insert(&conn, &make_event("evt-1", "pedidos", 1)).unwrap();
        Outbox::insert(&conn, &make_event("evt-2", "usuarios", 1)).unwrap();

        let rows = Outbox::fetch_unpublished(&conn, "pedidos", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].store, "pedidos");
    }
}

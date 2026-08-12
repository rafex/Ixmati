use rusqlite::Connection;

/// Durable record of a delete. Payload rows are intentionally removed, but
/// the version and timestamp remain available to offline merge/split tools.
pub struct Tombstones;

impl Tombstones {
    pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _tombstones (
                entity      TEXT NOT NULL,
                key         TEXT NOT NULL,
                version     INTEGER NOT NULL,
                deleted_at  TEXT NOT NULL DEFAULT (datetime('now')),
                event_id    TEXT,
                PRIMARY KEY (entity, key)
            );
            CREATE INDEX IF NOT EXISTS idx_tombstones_entity_key_version
                ON _tombstones(entity, key, version);",
        )
    }

    pub fn record(
        conn: &Connection,
        entity: &str,
        key: &str,
        version: u64,
        event_id: &str,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO _tombstones (entity, key, version, deleted_at, event_id)
             VALUES (?1, ?2, ?3, datetime('now'), ?4)
             ON CONFLICT(entity, key) DO UPDATE SET
                 version = excluded.version,
                 deleted_at = excluded.deleted_at,
                 event_id = excluded.event_id
             WHERE excluded.version >= _tombstones.version",
            rusqlite::params![entity, key, version as i64, event_id],
        )?;
        Ok(())
    }

    pub fn clear_if_older(
        conn: &Connection,
        entity: &str,
        key: &str,
        version: u64,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "DELETE FROM _tombstones WHERE entity = ?1 AND key = ?2 AND version <= ?3",
            rusqlite::params![entity, key, version as i64],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tombstone_is_monotonic() {
        let conn = Connection::open_in_memory().unwrap();
        Tombstones::ensure_schema(&conn).unwrap();
        Tombstones::record(&conn, "pedido", "p1", 3, "e3").unwrap();
        Tombstones::record(&conn, "pedido", "p1", 2, "e2").unwrap();
        let version: i64 = conn
            .query_row(
                "SELECT version FROM _tombstones WHERE entity='pedido' AND key='p1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 3);
    }
}

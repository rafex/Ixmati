use rusqlite::{Connection, Statement};
use std::time::Duration;

pub struct StatusQuery;

// Must stay below the 30ms polling interval/deadline budget. A multi-second
// busy timeout lets hundreds of concurrent durable-ACK polls occupy the
// blocking pool past WRITE_COMMITTED_TIMEOUT_MS; a short retry is safer than
// turning transient SQLite contention into a guaranteed PENDING response.
const STATUS_BUSY_TIMEOUT: Duration = Duration::from_millis(25);

const STATUS_SQL: &str = "SELECT idempotency_key, store, entity, key, version, applied_at
             FROM _idempotency
             WHERE store = ?1 AND idempotency_key = ?2";

impl StatusQuery {
    pub fn query(
        db_path: &str,
        store: &str,
        idempotency_key: &str,
    ) -> rusqlite::Result<WriteStatus> {
        let conn = Self::open(db_path)?;

        Self::query_connection(&conn, store, idempotency_key)
    }

    /// Open the read-side connection used while waiting for a durable commit.
    ///
    /// The API can poll the same SQLite file while the writer is committing a
    /// batch.  Without a busy timeout, a transient read/write lock was
    /// interpreted by the caller as "not applied yet", adding needless
    /// polling latency and hiding the actual SQLite contention.  `query_only`
    /// also makes the read-side contract explicit and prevents an API process
    /// from accidentally becoming a second writer.
    pub fn open(db_path: &str) -> rusqlite::Result<Connection> {
        let conn = Connection::open(db_path)?;
        conn.busy_timeout(STATUS_BUSY_TIMEOUT)?;
        conn.execute_batch("PRAGMA query_only = ON;")?;
        Ok(conn)
    }

    pub fn query_connection(
        conn: &Connection,
        store: &str,
        idempotency_key: &str,
    ) -> rusqlite::Result<WriteStatus> {
        let mut stmt = conn.prepare(STATUS_SQL)?;

        Self::query_statement(&mut stmt, store, idempotency_key)
    }

    /// Query a prepared statement so a durable-ACK poll does not recompile
    /// the same lookup every 30ms.
    pub fn query_statement(
        stmt: &mut Statement<'_>,
        store: &str,
        idempotency_key: &str,
    ) -> rusqlite::Result<WriteStatus> {
        let mut rows = stmt.query((store, idempotency_key))?;

        match rows.next()? {
            Some(row) => {
                let ik: String = row.get(0)?;
                let st: String = row.get(1)?;
                let entity: String = row.get(2)?;
                let key: String = row.get(3)?;
                let version: i64 = row.get(4)?;
                let applied_at: String = row.get(5)?;

                Ok(WriteStatus::Applied {
                    idempotency_key: ik,
                    store: st,
                    entity,
                    key,
                    version: version as u64,
                    applied_at,
                })
            }
            None => Ok(WriteStatus::Pending {
                store: store.into(),
                idempotency_key: idempotency_key.into(),
            }),
        }
    }
}

#[derive(Debug)]
pub enum WriteStatus {
    Pending {
        store: String,
        idempotency_key: String,
    },
    Applied {
        idempotency_key: String,
        store: String,
        entity: String,
        key: String,
        version: u64,
        applied_at: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_pending_returns_pending() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _idempotency (
                idempotency_key TEXT NOT NULL,
                store           TEXT NOT NULL,
                entity          TEXT NOT NULL,
                key             TEXT NOT NULL,
                version         INTEGER NOT NULL,
                applied_at      TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (store, idempotency_key)
            );",
        )
        .unwrap();

        let status = StatusQuery::query("file::memory:?cache=shared", "pedidos", "ik-nonexistent");

        assert!(status.is_err());
    }

    #[test]
    fn open_configures_read_side_connection() {
        let path =
            std::env::temp_dir().join(format!("ixmati-status-test-{}.db", uuid::Uuid::new_v4()));
        let path = path.to_str().unwrap();
        let conn = StatusQuery::open(path).unwrap();

        let timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, STATUS_BUSY_TIMEOUT.as_millis() as i64);

        let query_only: i64 = conn
            .query_row("PRAGMA query_only", [], |row| row.get(0))
            .unwrap();
        assert_eq!(query_only, 1);

        drop(conn);
        let _ = std::fs::remove_file(path);
    }
}

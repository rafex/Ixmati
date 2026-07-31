use rusqlite::Connection;

pub struct StatusQuery;

impl StatusQuery {
    pub fn query(
        db_path: &str,
        store: &str,
        idempotency_key: &str,
    ) -> rusqlite::Result<WriteStatus> {
        let conn = Connection::open(db_path)?;

        let mut stmt = conn.prepare(
            "SELECT idempotency_key, store, entity, key, version, applied_at
             FROM _idempotency
             WHERE store = ?1 AND idempotency_key = ?2",
        )?;

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
}

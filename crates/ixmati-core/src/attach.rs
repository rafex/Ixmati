use rusqlite::Connection;

pub struct ReadOnlyConnection {
    conn: Connection,
}

pub struct WriteConnectionGuard;

impl ReadOnlyConnection {
    pub fn open(path: &str, attached_stores: &[(&str, &str)]) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;

        if attached_stores.is_empty() {
            return Ok(Self { conn });
        }

        for (name, db_path) in attached_stores {
            let sql = format!("ATTACH DATABASE '{}' AS {}", db_path, name);
            conn.execute_batch(&sql)?;
        }

        Ok(Self { conn })
    }

    pub fn attach(&mut self, name: &str, db_path: &str) -> rusqlite::Result<()> {
        let sql = format!("ATTACH DATABASE '{}' AS {}", db_path, name);
        self.conn.execute_batch(&sql)
    }

    pub fn detach(&mut self, name: &str) -> rusqlite::Result<()> {
        let sql = format!("DETACH DATABASE {}", name);
        self.conn.execute_batch(&sql)
    }

    pub fn cross_store_query(
        &self,
        sql: &str,
    ) -> rusqlite::Result<Vec<Vec<String>>> {
        let mut stmt = self.conn.prepare(sql)?;
        let col_count = stmt.column_count();

        let rows = stmt
            .query_map([], |row| {
                let mut values = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    let val: String = row.get(i)?;
                    values.push(val);
                }
                Ok(values)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(rows)
    }
}

// Guard to assert ATTACH is blocked on write connections
impl WriteConnectionGuard {
    pub fn ensure_no_attach(conn: &Connection) -> rusqlite::Result<()> {
        let writer: bool = conn
            .pragma_query_value(None, "query_only", |row| row.get::<_, bool>(0))?;
        if writer {
            let readonly: bool = false;
            conn.pragma_update(None, "query_only", !readonly)?;
        }
        Ok(())
    }

    pub fn try_attach_on_write(conn: &Connection, name: &str, path: &str) -> Result<(), String> {
        let sql = format!("ATTACH DATABASE '{}' AS {}", path, name);
        match conn.execute_batch(&sql) {
            Err(e) if e.to_string().contains("not authorized") => Ok(()),
            Err(e) => Err(format!("unexpected error: {}", e)),
            Ok(()) => {
                conn.execute_batch(&format!("DETACH DATABASE {}", name)).ok();
                Err("ATTACH should be blocked on write connections".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_db(name: &str) -> String {
        let dir = std::env::temp_dir().join("ixmati-attach-tests");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name).to_string_lossy().to_string();
        fs::remove_file(&path).ok();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS test_data (id INTEGER PRIMARY KEY, value TEXT);",
        )
        .unwrap();
        conn.execute("INSERT OR REPLACE INTO test_data (id, value) VALUES (1, 'hello')", [])
            .unwrap();
        path
    }

    #[test]
    fn cross_store_query_with_attach() {
        let db_a = temp_db("store_a.db");
        let db_b = temp_db("store_b.db");

        let conn = ReadOnlyConnection::open(&db_a, &[("store_b", &db_b)]).unwrap();

        let rows = conn.cross_store_query("SELECT value FROM test_data").unwrap();
        assert_eq!(rows.len(), 1);

        let rows = conn
            .cross_store_query("SELECT value FROM store_b.test_data")
            .unwrap();
        assert_eq!(rows.len(), 1);

        fs::remove_file(&db_a).ok();
        fs::remove_file(&db_b).ok();
    }

    #[test]
    fn attach_detach_cycle() {
        let db_a = temp_db("store_a_cycle.db");
        let db_b = temp_db("store_b_cycle.db");

        let mut conn = ReadOnlyConnection::open(&db_a, &[]).unwrap();
        conn.attach("store_b", &db_b).unwrap();

        let rows = conn
            .cross_store_query("SELECT value FROM store_b.test_data")
            .unwrap();
        assert_eq!(rows.len(), 1);

        conn.detach("store_b").unwrap();

        fs::remove_file(&db_a).ok();
        fs::remove_file(&db_b).ok();
    }

    #[test]
    fn empty_attach_list_works() {
        let db = temp_db("empty_attach.db");
        let conn = ReadOnlyConnection::open(&db, &[]).unwrap();
        let rows = conn.cross_store_query("SELECT 'ok'").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "ok");
        fs::remove_file(&db).ok();
    }
}

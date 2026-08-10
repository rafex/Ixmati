//! Apertura de conexiones SQLite con los PRAGMAs que todo el crate necesita
//! (ver DEC-0001, DEC-0048/TASK-VAL-0017).
//!
//! `busy_timeout` faltaba en todas las conexiones que abre `ixmati-writer`
//! (solo estaba en el backend de cache SQLite, no acá) — bajo carga
//! concurrente entre el loop principal (`Arc<Mutex<Connection>>`) y las
//! conexiones ad-hoc de `event_publisher.rs`, eso causaba errores
//! inmediatos de "database is locked" en vez de esperar el tiempo
//! configurado, y precedió a los dos OOM-kill del writer documentados en
//! DEC-0048.

use rusqlite::Connection;

/// Milisegundos que SQLite espera antes de devolver `SQLITE_BUSY` cuando
/// otra conexión tiene el write lock. Configurable para tests/tuning.
const DEFAULT_BUSY_TIMEOUT_MS: u32 = 5000;

pub fn open_with_pragmas(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    apply_pragmas(&conn)?;
    Ok(conn)
}

pub fn apply_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL;
         PRAGMA busy_timeout={DEFAULT_BUSY_TIMEOUT_MS};"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_with_pragmas_sets_busy_timeout() {
        let path = std::env::temp_dir().join(format!(
            "ixmati-db-pragma-test-{}.db",
            uuid::Uuid::new_v4()
        ));
        let path_str = path.to_str().unwrap();

        let conn = open_with_pragmas(path_str).unwrap();
        let busy_timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy_timeout, DEFAULT_BUSY_TIMEOUT_MS as i64);

        drop(conn);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}

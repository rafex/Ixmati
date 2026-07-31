use chrono::Utc;
use ixmati_core::WriteEnvelope;
use ixmati_writer::write_engine::WriteEngine;
use rusqlite::Connection;
use serde_json::json;
use std::fs;

fn temp_db(name: &str) -> String {
    let dir = std::env::temp_dir().join("ixmati-crash-tests");
    fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().to_string()
}

fn make_cmd(store: &str, key: &str, version: u64, idem: &str) -> WriteEnvelope {
    WriteEnvelope {
        op: "upsert".into(),
        store: store.into(),
        entity: "pedido".into(),
        key: key.into(),
        version,
        ts: Utc::now().to_rfc3339(),
        idempotency_key: idem.into(),
        ack_mode: "committed".into(),
        payload: json!({"total": 100.0, "estado": "pendiente"}),
    }
}

#[test]
fn crash_recovery_no_data_loss() {
    let db_path = temp_db("crash_no_loss.db");
    let store_name = "pedidos";

    // Phase 1: write data normally
    {
        let mut conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        WriteEngine::ensure_schema(&conn, store_name).unwrap();

        let cmds: Vec<_> = (1..=10)
            .map(|i| make_cmd("pedidos", &format!("ped_{}", i), 1, &format!("ik-{}", i)))
            .collect();

        let result = WriteEngine::process_batch(&mut conn, &cmds).unwrap();
        assert_eq!(result.committed, 10);
        // Connection dropped here (simulates crash)
    }

    // Phase 2: reopen and verify all data present
    {
        let conn = Connection::open(&db_path).unwrap();
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM _idempotency WHERE store = ?1")
            .unwrap();
        let count: i64 = stmt.query_row(["pedidos"], |row| row.get(0)).unwrap();
        assert_eq!(count, 10, "all 10 records must survive crash");

        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM payload_pedidos")
            .unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 10, "all 10 entities must survive crash");
    }

    fs::remove_file(&db_path).ok();
}

#[test]
fn crash_during_batch_atomicity() {
    let db_path = temp_db("crash_batch_atomic.db");
    let store_name = "pedidos";

    {
        let mut conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        WriteEngine::ensure_schema(&conn, store_name).unwrap();

        let cmds: Vec<_> = (1..=5)
            .map(|i| make_cmd("pedidos", &format!("ped_{}", i), 1, &format!("ik-{}", i)))
            .collect();

        let result = WriteEngine::process_batch(&mut conn, &cmds).unwrap();
        assert_eq!(result.committed, 5);
        assert_eq!(result.events.len(), 5);
    }

    {
        let conn = Connection::open(&db_path).unwrap();

        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM _idempotency WHERE store = ?1")
            .unwrap();
        let idem_count: i64 = stmt.query_row(["pedidos"], |row| row.get(0)).unwrap();
        assert_eq!(idem_count, 5);

        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM _outbox WHERE store = ?1")
            .unwrap();
        let outbox_count: i64 = stmt.query_row(["pedidos"], |row| row.get(0)).unwrap();
        assert_eq!(outbox_count, 5, "all 5 events must be in outbox");

        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM payload_pedidos")
            .unwrap();
        let payload_count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(payload_count, 5);
    }

    fs::remove_file(&db_path).ok();
}

#[test]
fn wal_checkpoint_preserves_data() {
    let db_path = temp_db("crash_wal.db");
    let store_name = "pedidos";

    {
        let mut conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        WriteEngine::ensure_schema(&conn, store_name).unwrap();

        let cmds: Vec<_> = (1..=3)
            .map(|i| {
                make_cmd(
                    "pedidos",
                    &format!("ped_{}", i),
                    1,
                    &format!("ik-wal-{}", i),
                )
            })
            .collect();

        WriteEngine::process_batch(&mut conn, &cmds).unwrap();

        // Force WAL checkpoint
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
    }

    {
        let conn = Connection::open(&db_path).unwrap();
        let mut stmt = conn
            .prepare("SELECT key, version FROM _idempotency WHERE store = ?1 ORDER BY applied_at")
            .unwrap();
        let rows: Vec<(String, i64)> = stmt
            .query_map(["pedidos"], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, "ped_1");
        assert_eq!(rows[0].1, 1);
        assert_eq!(rows[1].0, "ped_2");
        assert_eq!(rows[1].1, 1);
        assert_eq!(rows[2].0, "ped_3");
        assert_eq!(rows[2].1, 1);
    }

    fs::remove_file(&db_path).ok();
}

#[test]
fn version_order_preserved_after_recovery() {
    let db_path = temp_db("crash_version_order.db");
    let store_name = "pedidos";

    {
        let mut conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        WriteEngine::ensure_schema(&conn, store_name).unwrap();

        // Write versions 1, 2, 3 for same key
        for v in 1..=3 {
            let cmds = vec![make_cmd(
                "pedidos",
                "ped_recurrent",
                v,
                &format!("ik-v{}", v),
            )];
            WriteEngine::process_batch(&mut conn, &cmds).unwrap();
        }
    }

    {
        let conn = Connection::open(&db_path).unwrap();
        let v: i64 = conn
            .query_row(
                "SELECT MAX(version) FROM _idempotency WHERE store = ?1 AND key = ?2",
                ("pedidos", "ped_recurrent"),
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v, 3, "version 3 must be the latest after recovery");
    }

    fs::remove_file(&db_path).ok();
}

#[test]
fn empty_db_after_clean_removal() {
    let db_path = temp_db("crash_empty.db");

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        WriteEngine::ensure_schema(&conn, "vacio").unwrap();
    }

    // Delete the db file (simulates intentional reset)
    fs::remove_file(&db_path).ok();

    {
        let conn = Connection::open(&db_path).unwrap();
        WriteEngine::ensure_schema(&conn, "vacio").unwrap();

        let mut stmt = conn.prepare("SELECT COUNT(*) FROM _idempotency").unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    fs::remove_file(&db_path).ok();
}

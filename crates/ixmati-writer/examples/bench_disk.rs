//! Benchmark de `process_batch` sobre SQLite en disco real (WAL +
//! synchronous=NORMAL, vía `ixmati_writer::db::open_with_pragmas`), sin
//! ninguna dependencia de MQTT/cache-server/red — mapeo de puntos calientes
//! (ver conversación TASK-VAL-0018): separa el costo real de I/O a disco
//! del costo del resto de la tubería (loop de consumo, `cache_sync`,
//! tokio). `bench.rs` (SQLite en memoria) ya descartó la lógica SQL en sí
//! como cuello de botella (15k-26k commits/s); esto prueba la hipótesis
//! del fsync/WAL de forma aislada.

use std::time::Instant;

use ixmati_core::WriteEnvelope;

const NUM_ITERATIONS: usize = 2000;
const BATCH_SIZE: usize = 100;

fn main() {
    println!("=== Ixmati Disk I/O Benchmark (WAL + synchronous=NORMAL) ===\n");

    bench_single_writes_on_disk();
    bench_batches_on_disk();

    println!("\n=== Benchmark Complete ===");
}

fn tmp_db_path(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ixmati-bench-disk-{}-{}.db",
        name,
        uuid::Uuid::new_v4()
    ));
    p.to_str().unwrap().to_string()
}

fn cleanup(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}-wal"));
    let _ = std::fs::remove_file(format!("{path}-shm"));
}

fn bench_single_writes_on_disk() {
    let path = tmp_db_path("single");
    let mut conn = ixmati_writer::db::open_with_pragmas(&path).unwrap();
    ixmati_writer::write_engine::WriteEngine::ensure_schema(&conn, "bench").unwrap();

    let start = Instant::now();
    let mut committed = 0usize;
    for i in 0..NUM_ITERATIONS {
        let cmd = WriteEnvelope {
            op: "upsert".into(),
            store: "bench".into(),
            entity: "item".into(),
            key: format!("key_{}", i),
            version: 1,
            ts: "2026-07-31T00:00:00Z".into(),
            payload: serde_json::json!({"value": i}),
            idempotency_key: format!("idem_{}", i),
            ack_mode: "accepted".into(),
        };
        if let Ok(result) =
            ixmati_writer::write_engine::WriteEngine::process_batch(&mut conn, &[cmd])
        {
            committed += result.committed;
        }
    }
    let elapsed = start.elapsed();

    println!(
        "Disco, 1 comando/tx: {} commits en {:.2?} -> {:.0} commits/s ({:.3}ms/commit)",
        committed,
        elapsed,
        committed as f64 / elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 / committed as f64,
    );

    drop(conn);
    cleanup(&path);
}

fn bench_batches_on_disk() {
    let path = tmp_db_path("batch");
    let mut conn = ixmati_writer::db::open_with_pragmas(&path).unwrap();
    ixmati_writer::write_engine::WriteEngine::ensure_schema(&conn, "bench").unwrap();

    let num_batches = NUM_ITERATIONS / BATCH_SIZE;
    let start = Instant::now();
    let mut total_committed = 0usize;
    for b in 0..num_batches {
        let cmds: Vec<WriteEnvelope> = (0..BATCH_SIZE)
            .map(|i| WriteEnvelope {
                op: "upsert".into(),
                store: "bench".into(),
                entity: "batch_item".into(),
                key: format!("b{}_k{}", b, i),
                version: 1,
                ts: "2026-07-31T00:00:00Z".into(),
                payload: serde_json::json!({"batch": b, "index": i}),
                idempotency_key: format!("batch_idem_{}_{}", b, i),
                ack_mode: "accepted".into(),
            })
            .collect();

        if let Ok(result) =
            ixmati_writer::write_engine::WriteEngine::process_batch(&mut conn, &cmds)
        {
            total_committed += result.committed;
        }
    }
    let elapsed = start.elapsed();
    let total_writes = num_batches * BATCH_SIZE;

    println!(
        "Disco, batch de {}: {} writes en {:.2?} -> {:.0} writes/s ({} committed, {:.3}ms/batch)",
        BATCH_SIZE,
        total_writes,
        elapsed,
        total_writes as f64 / elapsed.as_secs_f64(),
        total_committed,
        elapsed.as_secs_f64() * 1000.0 / num_batches as f64,
    );

    drop(conn);
    cleanup(&path);
}

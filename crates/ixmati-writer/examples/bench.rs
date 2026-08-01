use std::time::Instant;

use ixmati_core::WriteEnvelope;
use rusqlite::Connection;

const NUM_ITERATIONS: usize = 1000;

fn main() {
    println!("=== Ixmati Throughput Benchmark ===\n");

    bench_write_engine();
    bench_batch_processing();
    bench_dedup_check();
    bench_outbox_insert();
    bench_envelope_serde();

    println!("\n=== Benchmark Complete ===");
}

fn bench_write_engine() {
    let mut conn = Connection::open_in_memory().unwrap();
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
            committed += result.committed
        }
    }
    let elapsed = start.elapsed();

    println!(
        "Write Engine: {} commits in {:.2?} → {:.0} commits/s",
        committed,
        elapsed,
        committed as f64 / elapsed.as_secs_f64()
    );
}

fn bench_batch_processing() {
    let mut conn = Connection::open_in_memory().unwrap();
    ixmati_writer::write_engine::WriteEngine::ensure_schema(&conn, "bench").unwrap();

    let batch_size = 100;
    let num_batches = NUM_ITERATIONS / batch_size;

    let start = Instant::now();
    let mut total_committed = 0usize;
    for b in 0..num_batches {
        let cmds: Vec<WriteEnvelope> = (0..batch_size)
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
            total_committed += result.committed
        }
    }
    let elapsed = start.elapsed();
    let total_writes = num_batches * batch_size;

    println!(
        "Batch ({}x{}): {} writes in {:.2?} → {:.0} writes/s ({} committed)",
        num_batches,
        batch_size,
        total_writes,
        elapsed,
        total_writes as f64 / elapsed.as_secs_f64(),
        total_committed
    );
}

fn bench_dedup_check() {
    let mut conn = Connection::open_in_memory().unwrap();
    ixmati_writer::write_engine::WriteEngine::ensure_schema(&conn, "bench").unwrap();

    let start = Instant::now();
    for i in 0..NUM_ITERATIONS {
        let cmd = WriteEnvelope {
            op: "upsert".into(),
            store: "bench".into(),
            entity: "dup".into(),
            key: "same_key".into(),
            version: 1,
            ts: "2026-07-31T00:00:00Z".into(),
            payload: serde_json::json!({"iteration": i}),
            idempotency_key: "fixed_idem_key".into(),
            ack_mode: "accepted".into(),
        };
        let _ = ixmati_writer::write_engine::WriteEngine::process_batch(&mut conn, &[cmd]);
    }
    let elapsed = start.elapsed();

    println!(
        "Dedup: {} checks in {:.2?} → {:.0} checks/s",
        NUM_ITERATIONS,
        elapsed,
        NUM_ITERATIONS as f64 / elapsed.as_secs_f64()
    );
}

fn bench_outbox_insert() {
    let conn = Connection::open_in_memory().unwrap();
    ixmati_writer::outbox::Outbox::ensure_schema(&conn).unwrap();

    let start = Instant::now();
    for i in 0..NUM_ITERATIONS {
        let event = ixmati_core::EventEnvelope {
            event_id: format!("evt_{}", i),
            event_type: "created".into(),
            store: "bench".into(),
            entity: "item".into(),
            key: format!("key_{}", i),
            version: 1,
            occurred_at: "2026-07-31T00:00:00Z".into(),
            outbox_seq: i as u64,
            payload: serde_json::json!({"iteration": i}),
        };
        let _ = ixmati_writer::outbox::Outbox::insert(&conn, &event);
    }
    let elapsed = start.elapsed();

    println!(
        "Outbox: {} inserts in {:.2?} → {:.0} events/s",
        NUM_ITERATIONS,
        elapsed,
        NUM_ITERATIONS as f64 / elapsed.as_secs_f64()
    );
}

fn bench_envelope_serde() {
    let envelope = WriteEnvelope {
        op: "upsert".into(),
        store: "bench".into(),
        entity: "item".into(),
        key: "serde_key".into(),
        version: 1,
        ts: "2026-07-31T00:00:00Z".into(),
        payload: serde_json::json!({"name": "test", "count": 42, "tags": ["a", "b", "c"]}),
        idempotency_key: "serde_idem".into(),
        ack_mode: "accepted".into(),
    };

    let n = NUM_ITERATIONS * 10;
    let start = Instant::now();
    for _ in 0..n {
        let json = serde_json::to_vec(&envelope).unwrap();
        let _: WriteEnvelope = serde_json::from_slice(&json).unwrap();
    }
    let elapsed = start.elapsed();

    println!(
        "Serde: {} roundtrips in {:.2?} → {:.0} roundtrips/s",
        n,
        elapsed,
        n as f64 / elapsed.as_secs_f64()
    );
}

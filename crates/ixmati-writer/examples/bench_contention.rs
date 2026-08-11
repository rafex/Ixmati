//! Reproduce de forma aislada la hipótesis de contención entre la conexión
//! principal del writer y las conexiones ad-hoc de `event_publisher.rs`
//! sobre el MISMO archivo SQLite (ver conversación TASK-VAL-0018, tras
//! `bench_disk.rs` descartar que SQLite en sí, en una sola conexión, sea
//! lento). Sin MQTT/cache-server/red — dos tareas tokio compitiendo por el
//! mismo archivo, midiendo cuánto cae el throughput del "writer" cuando el
//! "publicador" compite por el lock de escritura en paralelo.

use ixmati_core::WriteEnvelope;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const DURATION_SECS: u64 = 10;
const BATCH_SIZE: usize = 100;

fn tmp_db_path(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ixmati-bench-contention-{}-{}.db",
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

#[tokio::main]
async fn main() {
    println!("=== Ixmati Contention Benchmark (writer vs. publisher sobre el mismo archivo) ===\n");

    bench_writer_alone().await;
    bench_writer_with_contending_publisher().await;

    println!("\n=== Benchmark Complete ===");
}

/// Baseline: solo el "writer" escribiendo, sin nadie más tocando el archivo.
async fn bench_writer_alone() {
    let path = tmp_db_path("alone");
    {
        let conn = ixmati_writer::db::open_with_pragmas(&path).unwrap();
        ixmati_writer::write_engine::WriteEngine::ensure_schema(&conn, "bench").unwrap();
    }

    let committed = Arc::new(AtomicUsize::new(0));
    let batches = Arc::new(AtomicUsize::new(0));
    let path_clone = path.clone();
    let committed_clone = Arc::clone(&committed);
    let batches_clone = Arc::clone(&batches);

    let deadline = Instant::now() + Duration::from_secs(DURATION_SECS);
    let writer_task = tokio::task::spawn_blocking(move || {
        let mut conn = ixmati_writer::db::open_with_pragmas(&path_clone).unwrap();
        let mut seq = 0usize;
        while Instant::now() < deadline {
            let cmds: Vec<WriteEnvelope> = (0..BATCH_SIZE)
                .map(|i| {
                    seq += 1;
                    WriteEnvelope {
                        op: "upsert".into(),
                        store: "bench".into(),
                        entity: "item".into(),
                        key: format!("k{}_{}", seq, i),
                        version: 1,
                        ts: "2026-07-31T00:00:00Z".into(),
                        payload: serde_json::json!({"seq": seq}),
                        idempotency_key: format!("idem_{}_{}", seq, i),
                        ack_mode: "accepted".into(),
                    }
                })
                .collect();
            if let Ok(result) =
                ixmati_writer::write_engine::WriteEngine::process_batch(&mut conn, &cmds)
            {
                committed_clone.fetch_add(result.committed, Ordering::Relaxed);
            }
            batches_clone.fetch_add(1, Ordering::Relaxed);
        }
    });

    writer_task.await.unwrap();

    println!(
        "Solo writer: {} commits, {} batches en {}s -> {:.1} commits/s",
        committed.load(Ordering::Relaxed),
        batches.load(Ordering::Relaxed),
        DURATION_SECS,
        committed.load(Ordering::Relaxed) as f64 / DURATION_SECS as f64,
    );

    cleanup(&path);
}

/// Igual, pero con una segunda tarea que abre SU PROPIA conexión al mismo
/// archivo y hace escrituras periódicas (simulando
/// `event_publisher::publish_unpublished` -> `mark_published_batch`, que
/// hace exactamente esto: abre conexión propia, hace un UPDATE, cierra).
async fn bench_writer_with_contending_publisher() {
    let path = tmp_db_path("contended");
    {
        let conn = ixmati_writer::db::open_with_pragmas(&path).unwrap();
        ixmati_writer::write_engine::WriteEngine::ensure_schema(&conn, "bench").unwrap();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS dummy_outbox (id INTEGER PRIMARY KEY, marked INTEGER DEFAULT 0)").unwrap();
    }

    let committed = Arc::new(AtomicUsize::new(0));
    let batches = Arc::new(AtomicUsize::new(0));
    let publisher_writes = Arc::new(AtomicUsize::new(0));

    let deadline = Instant::now() + Duration::from_secs(DURATION_SECS);

    // "Publisher": cada 200ms (igual a PUBLISH_INTERVAL_MS default), abre
    // su PROPIA conexión y hace una escritura, como
    // event_publisher.rs::publish_unpublished hace hoy.
    let path_pub = path.clone();
    let publisher_writes_clone = Arc::clone(&publisher_writes);
    let publisher_task = tokio::task::spawn_blocking(move || {
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
            if let Ok(conn) = ixmati_writer::db::open_with_pragmas(&path_pub) {
                let _ = conn.execute("INSERT INTO dummy_outbox (marked) VALUES (1)", []);
                publisher_writes_clone.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    let path_writer = path.clone();
    let committed_clone = Arc::clone(&committed);
    let batches_clone = Arc::clone(&batches);
    let writer_task = tokio::task::spawn_blocking(move || {
        let mut conn = ixmati_writer::db::open_with_pragmas(&path_writer).unwrap();
        let mut seq = 0usize;
        while Instant::now() < deadline {
            let cmds: Vec<WriteEnvelope> = (0..BATCH_SIZE)
                .map(|i| {
                    seq += 1;
                    WriteEnvelope {
                        op: "upsert".into(),
                        store: "bench".into(),
                        entity: "item".into(),
                        key: format!("k{}_{}", seq, i),
                        version: 1,
                        ts: "2026-07-31T00:00:00Z".into(),
                        payload: serde_json::json!({"seq": seq}),
                        idempotency_key: format!("idem_{}_{}", seq, i),
                        ack_mode: "accepted".into(),
                    }
                })
                .collect();
            if let Ok(result) =
                ixmati_writer::write_engine::WriteEngine::process_batch(&mut conn, &cmds)
            {
                committed_clone.fetch_add(result.committed, Ordering::Relaxed);
            }
            batches_clone.fetch_add(1, Ordering::Relaxed);
        }
    });

    let (_, _) = tokio::join!(writer_task, publisher_task);

    println!(
        "Writer + publicador compitiendo: {} commits, {} batches, publicador hizo {} escrituras en {}s -> {:.1} commits/s",
        committed.load(Ordering::Relaxed),
        batches.load(Ordering::Relaxed),
        publisher_writes.load(Ordering::Relaxed),
        DURATION_SECS,
        committed.load(Ordering::Relaxed) as f64 / DURATION_SECS as f64,
    );

    cleanup(&path);
}

//! Reproduce el patrón EXACTO del loop principal de `main.rs`: comandos
//! llegando de a uno por un canal `mpsc` acotado, consumidos vía
//! `tokio::select!` contra un `interval` de flush, acumulados en un
//! `Batcher`, y comprometidos a SQLite en disco real vía `spawn_blocking`.
//! Sin MQTT/cache-server — un productor sintético empuja comandos al canal
//! tan rápido como puede, simulando el eventloop de rumqttc a máxima
//! velocidad. Objetivo: aislar si el costo de consumir 100 items UNO A UNO
//! a través de `select!`/`recv()` (en vez de en bloque, como hacen
//! `bench_disk.rs`/`bench_contention.rs`) explica la brecha entre esos
//! benchmarks (miles de commits/s) y el sistema en vivo (~30-40/s).

use ixmati_core::WriteEnvelope;
use ixmati_writer::batcher::Batcher;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};

const DURATION_SECS: u64 = 10;
const CHANNEL_CAPACITY: usize = 5000;
const BATCH_SIZE: usize = 100;
const BATCH_INTERVAL_MS: u64 = 50;

fn tmp_db_path(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("ixmati-bench-loop-{}-{}.db", name, uuid::Uuid::new_v4()));
    p.to_str().unwrap().to_string()
}

fn cleanup(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}-wal"));
    let _ = std::fs::remove_file(format!("{path}-shm"));
}

#[tokio::main]
async fn main() {
    println!("=== Ixmati Channel-Loop Benchmark (patrón exacto de main.rs) ===\n");

    let path = tmp_db_path("loop");
    {
        let conn = ixmati_writer::db::open_with_pragmas(&path).unwrap();
        ixmati_writer::write_engine::WriteEngine::ensure_schema(&conn, "bench").unwrap();
    }

    let (tx, mut rx) = mpsc::channel::<WriteEnvelope>(CHANNEL_CAPACITY);

    // Productor: empuja comandos tan rápido como puede, simulando el
    // eventloop de MQTT a máxima velocidad (sin límite de red real).
    let deadline = Instant::now() + Duration::from_secs(DURATION_SECS);
    let producer = tokio::spawn(async move {
        let mut seq = 0usize;
        loop {
            if Instant::now() >= deadline {
                break;
            }
            seq += 1;
            let cmd = WriteEnvelope {
                op: "upsert".into(),
                store: "bench".into(),
                entity: "item".into(),
                key: format!("k{seq}"),
                version: 1,
                ts: "2026-07-31T00:00:00Z".into(),
                payload: serde_json::json!({"seq": seq}),
                idempotency_key: format!("idem_{seq}"),
                ack_mode: "accepted".into(),
            };
            if tx.send(cmd).await.is_err() {
                break;
            }
        }
    });

    // Consumidor: EXACTAMENTE el mismo patrón select! + interval + Batcher
    // + spawn_blocking que main.rs.
    let conn = Arc::new(Mutex::new(
        ixmati_writer::db::open_with_pragmas(&path).unwrap(),
    ));
    let mut batcher = Batcher::new(BATCH_SIZE, BATCH_INTERVAL_MS);
    let mut flush_interval = tokio::time::interval(Duration::from_millis(BATCH_INTERVAL_MS));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let committed = Arc::new(AtomicUsize::new(0));
    let batches = Arc::new(AtomicUsize::new(0));

    let consumer_deadline = Instant::now() + Duration::from_secs(DURATION_SECS + 2);
    loop {
        if Instant::now() >= consumer_deadline {
            break;
        }
        tokio::select! {
            cmd = rx.recv() => {
                match cmd {
                    Some(cmd) => {
                        if let Some(batch) = batcher.push(cmd) {
                            process_batch(&conn, &batch, &committed, &batches).await;
                        }
                    }
                    None => break,
                }
            }
            _ = flush_interval.tick() => {
                if batcher.should_flush() && !batcher.is_empty() {
                    let batch = batcher.flush();
                    process_batch(&conn, &batch, &committed, &batches).await;
                }
            }
        }
    }

    let _ = producer.await;

    println!(
        "Loop completo (select!+interval+Batcher+spawn_blocking): {} commits, {} batches en {}s -> {:.1} commits/s",
        committed.load(Ordering::Relaxed),
        batches.load(Ordering::Relaxed),
        DURATION_SECS,
        committed.load(Ordering::Relaxed) as f64 / DURATION_SECS as f64,
    );

    cleanup(&path);
    println!("\n=== Benchmark Complete ===");
}

async fn process_batch(
    conn: &Arc<Mutex<rusqlite::Connection>>,
    batch: &ixmati_writer::Batch,
    committed: &Arc<AtomicUsize>,
    batches: &Arc<AtomicUsize>,
) {
    let conn = Arc::clone(conn);
    let cmds = batch.commands.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut guard = conn.blocking_lock();
        ixmati_writer::write_engine::WriteEngine::process_batch(&mut guard, &cmds)
    })
    .await
    .unwrap();

    if let Ok(result) = result {
        committed.fetch_add(result.committed, Ordering::Relaxed);
    }
    batches.fetch_add(1, Ordering::Relaxed);
}

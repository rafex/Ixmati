// bench_backends.rs — benchmark comparativo de 3 backends de cache
//
// Ejecutar con:
//   cargo test -p ixmati-cache --test bench_backends -- --nocapture
//   # Para FlashDB:
//   cargo test -p ixmati-cache --test bench_backends --features ixmati-cache/flashdb,ixmati-cache/flashdb -- --nocapture

use ixmati_cache::{CacheBackend, NoOpBackend, SqliteCacheBackend, RedbCacheBackend};
#[cfg(feature = "flashdb")]
use ixmati_cache::FlashDb;

use std::time::Instant;
use std::path::PathBuf;

fn tmp_path(name: &str) -> String {
    let mut p = PathBuf::from(std::env::temp_dir());
    p.push(format!("ixmati-bench-{}", name));
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_dir_all(&p);
    p.to_str().unwrap().to_string()
}

struct BenchResult {
    name: String,
    set_1000_ms: u128,
    get_1000_ms: u128,
    get_p50_us: u128,
    get_p99_us: u128,
    del_500_ms: u128,
    size_bytes: u64,
    init_ms: u128,
}

fn bench_backend(name: &str, backend: &dyn CacheBackend, path: &str) -> BenchResult {
    let payload = vec![42u8; 1024]; // 1KB values

    // SET 1000
    let t0 = Instant::now();
    for i in 0..1000 {
        backend.set("cache", &format!("k{}", i), "", &payload);
    }
    let set_elapsed = t0.elapsed().as_millis();

    // GET 1000 (measure individual latencies)
    let mut latencies = Vec::with_capacity(1000);
    let t0 = Instant::now();
    for i in 0..1000 {
        let tt = Instant::now();
        let _ = backend.get("cache", &format!("k{}", i), "");
        latencies.push(tt.elapsed().as_micros());
    }
    let get_elapsed = t0.elapsed().as_millis();
    latencies.sort();
    let p50 = latencies[500];
    let p99 = latencies[990];

    // DEL 500
    let t0 = Instant::now();
    for i in 0..500 {
        backend.del("cache", &format!("k{}", i), "");
    }
    let del_elapsed = t0.elapsed().as_millis();

    // Size
    let size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => {
            // For directory-based backends (FlashDB)
            dir_size(path)
        }
    };

    BenchResult {
        name: name.to_string(),
        set_1000_ms: set_elapsed,
        get_1000_ms: get_elapsed,
        get_p50_us: p50,
        get_p99_us: p99,
        del_500_ms: del_elapsed,
        size_bytes: size,
        init_ms: 0,
    }
}

fn dir_size(path: &str) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(m) = entry.metadata() {
                total += m.len();
            }
        }
    }
    total
}

fn print_result(r: &BenchResult) {
    println!("\n=== {} ===", r.name);
    println!("  Init:         {} ms", r.init_ms);
    println!("  SET 1000:     {} ms  ({:.0} writes/s)", r.set_1000_ms, 1000.0 / (r.set_1000_ms as f64 / 1000.0));
    println!("  GET 1000:     {} ms  ({:.0} reads/s)", r.get_1000_ms, 1000.0 / (r.get_1000_ms as f64 / 1000.0));
    println!("  GET p50:      {} µs", r.get_p50_us);
    println!("  GET p99:      {} µs", r.get_p99_us);
    println!("  DEL 500:      {} ms", r.del_500_ms);
    println!("  Size on disk: {} bytes ({:.1} KB)", r.size_bytes, r.size_bytes as f64 / 1024.0);
}

#[test]
fn bench_sqlite() {
    let path = tmp_path("bench-sqlite.db");
    let t0 = Instant::now();
    let backend = SqliteCacheBackend::new(&path).unwrap();
    let init_ms = t0.elapsed().as_millis();
    let mut r = bench_backend("SQLite", &backend, &path);
    r.init_ms = init_ms;
    print_result(&r);
}

#[test]
fn bench_redb() {
    let path = tmp_path("bench-redb.redb");
    let t0 = Instant::now();
    let backend = RedbCacheBackend::new(&path).unwrap();
    let init_ms = t0.elapsed().as_millis();
    let mut r = bench_backend("Redb", &backend, &path);
    r.init_ms = init_ms;
    print_result(&r);
}

#[cfg(feature = "flashdb")]
#[test]
fn bench_flashdb() {
    let path = tmp_path("bench-flashdb");
    let t0 = Instant::now();
    let backend = FlashDb::new(&path).unwrap();
    let init_ms = t0.elapsed().as_millis();
    let mut r = bench_backend("FlashDB", &backend, &path);
    r.init_ms = init_ms;
    print_result(&r);
}

#[test]
fn bench_noop() {
    let backend = NoOpBackend::new();
    let r = bench_backend("NoOp", &backend, "/dev/null");
    print_result(&r);
}

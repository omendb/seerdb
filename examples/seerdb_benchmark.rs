// Simple benchmark for seerdb
// Compare against fjall baseline (438k writes/sec)

use seerdb::{DB, DBOptions};
use std::time::Instant;
use tempfile::tempdir;

fn main() {
    println!("=== seerdb Performance Benchmark ===\n");

    let operations = 100_000;
    let value_size = 1024;

    println!("Operations: {}", operations);
    println!("Value size: {} bytes\n", value_size);

    // Setup
    let dir = tempdir().unwrap();
    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 256 * 1024 * 1024, // 256MB (large to avoid flushes during benchmark)
        wal_sync_policy: seerdb::wal::SyncPolicy::None, // Fast mode for benchmark
        ..Default::default()
    };

    let db = DB::open(options).expect("Failed to open database");

    // Workload 1: Sequential Writes
    println!("Workload 1: Sequential Writes ({} ops)", operations);
    let start = Instant::now();

    for i in 0..operations {
        let key = format!("key_{:010}", i);
        let value = vec![b'x'; value_size];
        db.put(key.as_bytes(), &value).unwrap();
    }

    let duration = start.elapsed();
    let throughput = operations as f64 / duration.as_secs_f64();
    let latency = duration.as_micros() as f64 / operations as f64;

    println!("  Time: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Latency: {:.2} us/op", latency);
    println!();

    // Workload 2: Random Reads
    println!("Workload 2: Random Reads ({} ops)", operations);
    let start = Instant::now();

    for i in 0..operations {
        let key = format!("key_{:010}", i);
        let _value = db.get(key.as_bytes()).unwrap();
    }

    let duration = start.elapsed();
    let throughput = operations as f64 / duration.as_secs_f64();
    let latency = duration.as_micros() as f64 / operations as f64;

    println!("  Time: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Latency: {:.2} us/op", latency);
    println!();

    // Workload 3: Mixed 50/50
    println!("Workload 3: Mixed 50/50 ({} ops)", operations);
    let start = Instant::now();

    for i in 0..operations {
        if i % 2 == 0 {
            // Write
            let key = format!("mix_{:010}", i);
            let value = vec![b'y'; value_size];
            db.put(key.as_bytes(), &value).unwrap();
        } else {
            // Read (from previous writes)
            let key = format!("key_{:010}", i / 2);
            let _value = db.get(key.as_bytes()).unwrap();
        }
    }

    let duration = start.elapsed();
    let throughput = operations as f64 / duration.as_secs_f64();
    let latency = duration.as_micros() as f64 / operations as f64;

    println!("  Time: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Latency: {:.2} us/op", latency);
    println!();

    // Stats
    println!("Database Stats:");
    println!("  Memtable entries: {}", db.memtable_len());
    println!("  Memtable size: {} bytes", db.memtable_size());

    println!("\n=== Comparison to Baselines ===");
    println!("Target (fjall):    438,000 writes/sec");
    println!("Target (RocksDB):  363,000 writes/sec");
    println!();
}

// Profile write path to identify bottlenecks
// Focused on pure sequential writes with timing breakdown

use seerdb::{DB, DBOptions};
use std::time::Instant;
use tempfile::tempdir;

const NUM_WRITES: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    println!("=== Write Path Profiling ===");
    println!("Operations: {}", NUM_WRITES);
    println!("Value size: {} bytes", VALUE_SIZE);
    println!();

    // Test 1: With WAL sync (realistic)
    println!("Test 1: With WAL sync (SyncData)");
    profile_writes(seerdb::SyncPolicy::SyncData, false);
    println!();

    // Test 2: Without WAL sync (isolate WAL overhead)
    println!("Test 2: Without WAL sync (None)");
    profile_writes(seerdb::SyncPolicy::None, false);
    println!();

    // Test 3: Large memtable (no flushes)
    println!("Test 3: Large memtable (no flushes)");
    profile_writes(seerdb::SyncPolicy::None, true);
    println!();
}

fn profile_writes(sync_policy: seerdb::SyncPolicy, large_memtable: bool) {
    let dir = tempdir().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = dir.path().to_path_buf();
    opts.wal_sync_policy = sync_policy;
    opts.background_compaction = false;
    opts.background_flush = false;

    if large_memtable {
        opts.memtable_capacity = 1024 * 1024 * 1024; // 1GB - avoid flushes
    } else {
        opts.memtable_capacity = 64 * 1024 * 1024; // 64MB - may trigger flushes
    }

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    let start = Instant::now();
    for i in 0..NUM_WRITES {
        let key = format!("key{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }
    let duration = start.elapsed();

    let throughput = NUM_WRITES as f64 / duration.as_secs_f64();
    let latency_us = duration.as_micros() as f64 / NUM_WRITES as f64;

    println!("  Time: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Latency: {:.2} µs/op", latency_us);

    // Get stats
    let stats = db.stats();
    println!(
        "  Logical bytes: {:.2} MB",
        stats.logical_bytes_written as f64 / 1_000_000.0
    );
    println!(
        "  Physical bytes: {:.2} MB",
        stats.physical_bytes_written as f64 / 1_000_000.0
    );
    if stats.logical_bytes_written > 0 {
        let write_amp = stats.physical_bytes_written as f64 / stats.logical_bytes_written as f64;
        println!("  Write amplification: {:.2}x", write_amp);
    }
}

// Analyze bloom filter false positive rate
// This helps understand if bloom filter is the bottleneck

use seerdb::{DBOptions, DB};
use std::time::Instant;
use tempfile::tempdir;

const NUM_KEYS: usize = 100_000;
const NUM_QUERIES: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    println!("=== Bloom Filter Analysis ===\n");

    // Setup
    let dir = tempdir().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = dir.path().to_path_buf();
    opts.memtable_capacity = 256 * 1024 * 1024;
    opts.wal_sync_policy = seerdb::SyncPolicy::None;
    opts.background_compaction = false;

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    println!(
        "Writing {} keys with pattern 'key00000000' to 'key{:08}'",
        NUM_KEYS,
        NUM_KEYS - 1
    );
    for i in 0..NUM_KEYS {
        let key = format!("key{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }

    println!("Flushing to SSTables...");
    db.flush().unwrap();
    println!();

    // Test 1: Query keys that DEFINITELY don't exist (different prefix)
    println!("Test 1: Different prefix (should be fast - no false positives)");
    let start = Instant::now();
    let mut found = 0;
    for i in 0..NUM_QUERIES {
        let key = format!("xxx{:08}", i); // Different prefix
        let result = db.get(key.as_bytes()).unwrap();
        if result.is_some() {
            found += 1;
        }
    }
    let duration = start.elapsed();
    let throughput = NUM_QUERIES as f64 / duration.as_secs_f64();
    println!("  Found: {} (should be 0)", found);
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!(
        "  Latency: {:.2} µs/op",
        duration.as_micros() as f64 / NUM_QUERIES as f64
    );
    println!();

    // Test 2: Query keys slightly outside range (similar prefix, might cause false positives)
    println!("Test 2: Outside range (similar prefix - may have false positives)");
    let start = Instant::now();
    let mut found = 0;
    for i in 0..NUM_QUERIES {
        let key = format!("key{:08}", NUM_KEYS + i); // Just outside range
        let result = db.get(key.as_bytes()).unwrap();
        if result.is_some() {
            found += 1;
        }
    }
    let duration = start.elapsed();
    let throughput = NUM_QUERIES as f64 / duration.as_secs_f64();
    println!("  Found: {} (should be 0)", found);
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!(
        "  Latency: {:.2} µs/op",
        duration.as_micros() as f64 / NUM_QUERIES as f64
    );
    println!();

    // Test 3: Query keys that exist (baseline)
    println!("Test 3: Keys that exist (baseline for comparison)");
    let start = Instant::now();
    let mut found = 0;
    for i in 0..NUM_QUERIES {
        let key_idx = (i * 7919) % NUM_KEYS;
        let key = format!("key{:08}", key_idx);
        let result = db.get(key.as_bytes()).unwrap();
        if result.is_some() {
            found += 1;
        }
    }
    let duration = start.elapsed();
    let throughput = NUM_QUERIES as f64 / duration.as_secs_f64();
    println!("  Found: {} (should be ~100000)", found);
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!(
        "  Latency: {:.2} µs/op",
        duration.as_micros() as f64 / NUM_QUERIES as f64
    );
    println!();

    // Analysis
    println!("=== Analysis ===");
    println!("If Test 1 (different prefix) is much faster than Test 2 (similar prefix),");
    println!("it indicates bloom filter false positives are causing unnecessary disk I/O.");
    println!();
    println!("If Test 2 is similar speed to Test 3 (existing keys), it suggests");
    println!("bloom filter is not effectively filtering non-existent keys.");
}

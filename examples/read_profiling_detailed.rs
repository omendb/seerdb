// Detailed read profiling to identify bottlenecks
// Measures bloom filter, cache, I/O, and decoding overhead

use seerdb::{DB, DBOptions};
use std::time::Instant;
use tempfile::tempdir;

const NUM_KEYS: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    println!("=== Detailed Read Profiling ===\n");

    // Setup: Write data and flush
    let dir = tempdir().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = dir.path().to_path_buf();
    opts.memtable_capacity = 256 * 1024 * 1024;
    opts.wal_sync_policy = seerdb::SyncPolicy::None;
    opts.background_compaction = false;

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    println!("Writing {} keys...", NUM_KEYS);
    for i in 0..NUM_KEYS {
        let key = format!("key{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }

    println!("Flushing to SSTables...");
    db.flush().unwrap();
    println!();

    // Test 1: Sequential reads (cache-friendly)
    println!("Test 1: Sequential Reads (cache-friendly)");
    test_sequential_reads(&db);
    println!();

    // Test 2: Random reads (cache-unfriendly)
    println!("Test 2: Random Reads (cache-unfriendly)");
    test_random_reads(&db);
    println!();

    // Test 3: Repeated reads (measure pure cache performance)
    println!("Test 3: Repeated Reads (cache hits)");
    test_repeated_reads(&db);
    println!();

    // Test 4: Non-existent keys (bloom filter effectiveness)
    println!("Test 4: Non-existent Keys (bloom filter test)");
    test_nonexistent_reads(&db);
    println!();

    // Test 5: Mixed existing + non-existing
    println!("Test 5: Mixed Existing + Non-existing (50/50)");
    test_mixed_reads(&db);
    println!();
}

fn test_sequential_reads(db: &DB) {
    let start = Instant::now();
    let mut found = 0;

    for i in 0..NUM_KEYS {
        let key = format!("key{:08}", i);
        let result = db.get(key.as_bytes()).unwrap();
        if result.is_some() {
            found += 1;
        }
    }

    let duration = start.elapsed();
    let throughput = NUM_KEYS as f64 / duration.as_secs_f64();
    let latency_us = duration.as_micros() as f64 / NUM_KEYS as f64;

    println!("  Found: {}/{}", found, NUM_KEYS);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Latency: {:.2} µs/op", latency_us);
}

fn test_random_reads(db: &DB) {
    let start = Instant::now();
    let mut found = 0;

    // Pseudo-random access pattern
    for i in 0..NUM_KEYS {
        let key_idx = (i * 7919) % NUM_KEYS;
        let key = format!("key{:08}", key_idx);
        let result = db.get(key.as_bytes()).unwrap();
        if result.is_some() {
            found += 1;
        }
    }

    let duration = start.elapsed();
    let throughput = NUM_KEYS as f64 / duration.as_secs_f64();
    let latency_us = duration.as_micros() as f64 / NUM_KEYS as f64;

    println!("  Found: {}/{}", found, NUM_KEYS);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Latency: {:.2} µs/op", latency_us);
}

fn test_repeated_reads(db: &DB) {
    // Read same 100 keys repeatedly to measure cache hit performance
    let num_iterations = 1000;
    let num_unique_keys = 100;
    let total_reads = num_iterations * num_unique_keys;

    let start = Instant::now();
    let mut found = 0;

    for _ in 0..num_iterations {
        for i in 0..num_unique_keys {
            let key = format!("key{:08}", i);
            let result = db.get(key.as_bytes()).unwrap();
            if result.is_some() {
                found += 1;
            }
        }
    }

    let duration = start.elapsed();
    let throughput = total_reads as f64 / duration.as_secs_f64();
    let latency_us = duration.as_micros() as f64 / total_reads as f64;

    println!("  Unique keys: {}", num_unique_keys);
    println!("  Iterations: {}", num_iterations);
    println!("  Total reads: {}", total_reads);
    println!("  Found: {}/{}", found, total_reads);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Latency: {:.2} µs/op", latency_us);
    println!("  (This measures cache hit performance)");
}

fn test_nonexistent_reads(db: &DB) {
    let start = Instant::now();
    let mut found = 0;
    let mut not_found = 0;

    // Query keys that don't exist
    for i in 0..NUM_KEYS {
        let key = format!("nonexist{:08}", i);
        let result = db.get(key.as_bytes()).unwrap();
        if result.is_some() {
            found += 1;
        } else {
            not_found += 1;
        }
    }

    let duration = start.elapsed();
    let throughput = NUM_KEYS as f64 / duration.as_secs_f64();
    let latency_us = duration.as_micros() as f64 / NUM_KEYS as f64;

    println!("  Found: {}", found);
    println!("  Not found: {}", not_found);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Latency: {:.2} µs/op", latency_us);
    println!("  (This measures bloom filter effectiveness)");
}

fn test_mixed_reads(db: &DB) {
    let start = Instant::now();
    let mut found = 0;
    let mut not_found = 0;

    // Alternate between existing and non-existing keys
    for i in 0..NUM_KEYS {
        let key = if i % 2 == 0 {
            format!("key{:08}", i / 2)
        } else {
            format!("nonexist{:08}", i / 2)
        };

        let result = db.get(key.as_bytes()).unwrap();
        if result.is_some() {
            found += 1;
        } else {
            not_found += 1;
        }
    }

    let duration = start.elapsed();
    let throughput = NUM_KEYS as f64 / duration.as_secs_f64();
    let latency_us = duration.as_micros() as f64 / NUM_KEYS as f64;

    println!("  Found: {}", found);
    println!("  Not found: {}", not_found);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Latency: {:.2} µs/op", latency_us);
}

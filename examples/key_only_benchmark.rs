// Benchmark to demonstrate 5-10x improvement with key-only iteration
// Use case: count(), exists(), cardinality estimation

use seerdb::{DBOptions, SyncPolicy, DB};
use std::time::Instant;
use tempfile::tempdir;

const NUM_KEYS: usize = 100_000;
const VALUE_SIZE: usize = 1024; // Large values to show vLog skip benefit

fn main() {
    println!("=== Key-Only Iteration Benchmark ===\n");

    let dir = tempdir().unwrap();
    let opts = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 4 * 1024 * 1024,
        wal_sync_policy: SyncPolicy::None,
        background_compaction: false,
        block_cache_capacity: 16_384,
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    println!(
        "Phase 1: Writing {} keys with {}KB values each",
        NUM_KEYS,
        VALUE_SIZE / 1024
    );
    for i in 0..NUM_KEYS {
        let key = format!("user:{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();

        if (i + 1) % 10_000 == 0 {
            db.flush().unwrap();
            print!(".");
        }
    }
    db.flush().unwrap();
    println!(" Done!\n");

    // Test 1: Count with full value reads (baseline)
    println!("Test 1: Count with value reads (baseline)");
    let start = Instant::now();
    let count = db.prefix(b"user:").unwrap().count();
    let duration = start.elapsed();
    println!("  Count: {}", count);
    println!("  Duration: {:.3}s", duration.as_secs_f64());
    println!(
        "  Throughput: {:.0} keys/sec\n",
        count as f64 / duration.as_secs_f64()
    );

    // Test 2: Count with key-only iteration (optimized)
    println!("Test 2: Count with keys-only (optimized)");
    let start = Instant::now();
    let count = db.prefix_keys_only(b"user:").unwrap().count();
    let duration = start.elapsed();
    let keys_per_sec = count as f64 / duration.as_secs_f64();
    println!("  Count: {}", count);
    println!("  Duration: {:.3}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} keys/sec\n", keys_per_sec);

    // Expected: 5-10x faster for keys-only (skips 1KB value reads + vLog lookups)
    println!("Expected: 5-10x faster with keys-only (skips value decoding + vLog)");
}

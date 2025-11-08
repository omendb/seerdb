// Benchmark to measure block cache hit rate
// This helps understand if low cache hits are the bottleneck

use seerdb::{DBOptions, DB};
use std::time::Instant;
use tempfile::tempdir;

const NUM_KEYS: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    println!("=== Block Cache Hit Rate Benchmark ===\n");

    // Setup
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

    // Test 1: Sequential reads (should have good cache locality)
    println!("Test 1: Sequential Reads");
    let start = Instant::now();
    let mut found = 0;

    for i in 0..NUM_KEYS {
        let key = format!("key{:08}", i);
        if db.get(key.as_bytes()).unwrap().is_some() {
            found += 1;
        }
    }

    let duration = start.elapsed();
    let throughput = NUM_KEYS as f64 / duration.as_secs_f64();

    println!("  Found: {}/{}", found, NUM_KEYS);
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Duration: {:.2}s", duration.as_secs_f64());

    // Get cache stats after sequential reads
    let stats = db.stats();
    println!("  Cache hits: {}", stats.cache_hits);
    println!("  Cache misses: {}", stats.cache_misses);
    println!("  Cache hit rate: {:.2}%", stats.cache_hit_rate * 100.0);
    println!();

    // Test 2: Random reads (should have worse cache locality)
    println!("Test 2: Random Reads");
    let start = Instant::now();
    let mut found = 0;

    for i in 0..NUM_KEYS {
        let key_idx = (i * 7919) % NUM_KEYS; // Pseudo-random
        let key = format!("key{:08}", key_idx);
        if db.get(key.as_bytes()).unwrap().is_some() {
            found += 1;
        }
    }

    let duration = start.elapsed();
    let throughput = NUM_KEYS as f64 / duration.as_secs_f64();

    println!("  Found: {}/{}", found, NUM_KEYS);
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Duration: {:.2}s", duration.as_secs_f64());

    // Get cache stats after random reads
    let stats = db.stats();
    println!("  Cache hits: {}", stats.cache_hits);
    println!("  Cache misses: {}", stats.cache_misses);
    println!("  Cache hit rate: {:.2}%", stats.cache_hit_rate * 100.0);
    println!();

    // Test 3: Repeated reads (should have very high cache hit rate)
    println!("Test 3: Repeated Reads (same 100 keys, 1000 times each)");
    let num_unique_keys = 100;
    let num_iterations = 1000;
    let total_reads = num_unique_keys * num_iterations;

    let start = Instant::now();
    let mut found = 0;

    for _ in 0..num_iterations {
        for i in 0..num_unique_keys {
            let key = format!("key{:08}", i);
            if db.get(key.as_bytes()).unwrap().is_some() {
                found += 1;
            }
        }
    }

    let duration = start.elapsed();
    let throughput = total_reads as f64 / duration.as_secs_f64();

    println!("  Found: {}/{}", found, total_reads);
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Duration: {:.2}s", duration.as_secs_f64());

    // Get cache stats after repeated reads
    let stats = db.stats();
    println!("  Cache hits: {}", stats.cache_hits);
    println!("  Cache misses: {}", stats.cache_misses);
    println!("  Cache hit rate: {:.2}%", stats.cache_hit_rate * 100.0);
    println!();

    // Summary
    println!("=== Summary ===");
    println!("Total operations: {}", total_reads + 2 * NUM_KEYS);
    println!("Total cache hits: {}", stats.cache_hits);
    println!("Total cache misses: {}", stats.cache_misses);
    println!("Overall cache hit rate: {:.2}%", stats.cache_hit_rate * 100.0);
    println!();
    println!("Expected behavior:");
    println!("- Sequential reads: Moderate hit rate (blocks loaded sequentially)");
    println!("- Random reads: Low hit rate (cache thrashing)");
    println!("- Repeated reads: High hit rate (same blocks repeatedly accessed)");
    println!();
    println!("If overall hit rate < 50%, cache is too small or eviction is poor");
    println!("If overall hit rate > 70%, cache is working well");
}

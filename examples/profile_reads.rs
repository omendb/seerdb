// Profile read path to identify bottlenecks
// Tests different scenarios to isolate performance issues

use seerdb::{DBOptions, DB};
use std::time::Instant;
use tempfile::tempdir;

const NUM_KEYS: usize = 100_000;
const NUM_READS: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    println!("=== Read Path Profiling ===");
    println!("Keys: {}", NUM_KEYS);
    println!("Read operations: {}", NUM_READS);
    println!("Value size: {} bytes", VALUE_SIZE);
    println!();

    // Test 1: Reads from memtable only (no SSTables)
    println!("Test 1: Memtable-only reads (all in memory)");
    test_memtable_reads();
    println!();

    // Test 2: Reads from SSTables (after flush)
    println!("Test 2: SSTable reads (after flush)");
    test_sstable_reads();
    println!();

    // Test 3: Check partition lookup overhead
    println!("Test 3: Partition lookup analysis");
    test_partition_overhead();
    println!();
}

fn test_memtable_reads() {
    let dir = tempdir().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = dir.path().to_path_buf();
    opts.memtable_capacity = 1024 * 1024 * 1024; // 1GB - keep everything in memtable
    opts.wal_sync_policy = seerdb::SyncPolicy::None;

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    // Write data
    println!("  Writing {} keys...", NUM_KEYS);
    for i in 0..NUM_KEYS {
        let key = format!("key{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }

    // Verify all in memtable
    let memtable_size = db.memtable_size();
    println!(
        "  Memtable size: {:.2} MB",
        memtable_size as f64 / 1_000_000.0
    );

    // Random reads
    println!("  Performing {} random reads...", NUM_READS);
    let start = Instant::now();
    for i in 0..NUM_READS {
        let key_idx = (i * 7919) % NUM_KEYS; // Pseudo-random
        let key = format!("key{:08}", key_idx);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some());
    }
    let duration = start.elapsed();

    let throughput = NUM_READS as f64 / duration.as_secs_f64();
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!(
        "  Latency: {:.2} µs/op",
        duration.as_micros() as f64 / NUM_READS as f64
    );
}

fn test_sstable_reads() {
    let dir = tempdir().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = dir.path().to_path_buf();
    opts.memtable_capacity = 256 * 1024 * 1024;
    opts.wal_sync_policy = seerdb::SyncPolicy::None;
    opts.background_compaction = false;

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    // Write data
    println!("  Writing {} keys...", NUM_KEYS);
    for i in 0..NUM_KEYS {
        let key = format!("key{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }

    // Flush to SSTables
    println!("  Flushing to SSTables...");
    db.flush().unwrap();

    let memtable_size = db.memtable_size();
    println!(
        "  Memtable size after flush: {:.2} MB",
        memtable_size as f64 / 1_000_000.0
    );

    // Check how many SSTables were created and their sizes
    let data_dir = dir.path();
    let mut sstable_paths = Vec::new();
    for entry in std::fs::read_dir(data_dir).unwrap() {
        let entry = entry.unwrap();
        if entry
            .path()
            .extension()
            .map(|s| s == "sst")
            .unwrap_or(false)
        {
            sstable_paths.push(entry.path());
        }
    }
    println!("  SSTables created: {}", sstable_paths.len());

    // Open and check SSTable lengths
    for (idx, path) in sstable_paths.iter().enumerate() {
        let mut sstable = seerdb::sstable::SSTable::open(path).unwrap();
        println!("  SSTable {}: {} entries", idx, sstable.len());
    }

    // Random reads from SSTables
    println!("  Performing {} random reads from SSTables...", NUM_READS);

    // First, test a few specific keys
    for test_idx in &[0, 1, 100, 1000, NUM_KEYS - 1] {
        let key = format!("key{:08}", test_idx);
        let result = db.get(key.as_bytes()).unwrap();
        if result.is_none() {
            println!("  WARNING: Key '{}' not found after flush!", key);
        }
    }

    let start = Instant::now();
    let mut found = 0;
    let mut not_found = 0;
    for i in 0..NUM_READS {
        let key_idx = (i * 7919) % NUM_KEYS; // Pseudo-random
        let key = format!("key{:08}", key_idx);
        let result = db.get(key.as_bytes()).unwrap();
        if result.is_some() {
            found += 1;
        } else {
            not_found += 1;
        }
    }
    let duration = start.elapsed();

    println!("  Found: {}, Not found: {}", found, not_found);

    let throughput = NUM_READS as f64 / duration.as_secs_f64();
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!(
        "  Latency: {:.2} µs/op",
        duration.as_micros() as f64 / NUM_READS as f64
    );
}

fn test_partition_overhead() {
    // This test checks if partitioning itself adds overhead
    let dir = tempdir().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = dir.path().to_path_buf();
    opts.memtable_capacity = 256 * 1024 * 1024;
    opts.wal_sync_policy = seerdb::SyncPolicy::None;

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    // Write some keys
    println!("  Writing {} keys...", 10_000);
    for i in 0..10_000 {
        let key = format!("key{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }

    // Test get() performance
    let test_key = b"key00005000";

    println!("  Testing single key lookup (10,000 iterations)...");
    let start = Instant::now();
    for _ in 0..10_000 {
        let result = db.get(test_key).unwrap();
        assert!(result.is_some());
    }
    let duration = start.elapsed();

    println!(
        "  Avg lookup time: {:.2} µs",
        duration.as_micros() as f64 / 10_000.0
    );
    println!(
        "  Throughput: {:.0} ops/sec",
        10_000.0 / duration.as_secs_f64()
    );
}

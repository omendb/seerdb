// Realistic profiling benchmark for seerdb
// - Smaller memtable to force flushes
// - Reads from SSTables (not just memtable)
// - WAL sync enabled (realistic durability)

use seerdb::{DBOptions, DB};
use std::time::Instant;
use tempfile::tempdir;

fn main() {
    println!("=== seerdb Profiling Benchmark (Realistic Workload) ===\n");

    let operations = 50_000;
    let value_size = 1024;

    println!("Operations: {}", operations);
    println!("Value size: {} bytes\n", value_size);

    // Setup - smaller memtable to force flushes
    let dir = tempdir().unwrap();
    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 4 * 1024 * 1024, // 4MB (will flush during benchmark)
        wal_sync_policy: seerdb::wal::SyncPolicy::SyncData, // Realistic durability
        ..Default::default()
    };

    let db = DB::open(options).expect("Failed to open database");

    // Phase 1: Write data to create SSTables
    println!("Phase 1: Writing data to create SSTables");
    let start = Instant::now();

    for i in 0..operations {
        let key = format!("key_{:010}", i);
        let value = vec![b'x'; value_size];
        db.put(key.as_bytes(), &value).unwrap();
    }

    db.flush().unwrap(); // Force flush
    let duration = start.elapsed();
    println!("  Write time: {:.2}s", duration.as_secs_f64());
    println!(
        "  Write throughput: {:.0} ops/sec",
        operations as f64 / duration.as_secs_f64()
    );
    println!();

    // Phase 2: Random reads from SSTables
    println!("Phase 2: Random reads from SSTables");
    let start = Instant::now();

    for i in 0..operations {
        let key = format!("key_{:010}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Key not found: {}", key);
    }

    let duration = start.elapsed();
    println!("  Read time: {:.2}s", duration.as_secs_f64());
    println!(
        "  Read throughput: {:.0} ops/sec",
        operations as f64 / duration.as_secs_f64()
    );
    println!();

    // Phase 3: Mixed workload (50/50 read/write)
    println!("Phase 3: Mixed workload (50/50)");
    let start = Instant::now();

    for i in 0..operations {
        if i % 2 == 0 {
            // Write new key
            let key = format!("mix_{:010}", i);
            let value = vec![b'y'; value_size];
            db.put(key.as_bytes(), &value).unwrap();
        } else {
            // Read existing key from SSTable
            let key = format!("key_{:010}", i / 2);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some());
        }
    }

    let duration = start.elapsed();
    println!("  Mixed time: {:.2}s", duration.as_secs_f64());
    println!(
        "  Mixed throughput: {:.0} ops/sec",
        operations as f64 / duration.as_secs_f64()
    );
    println!();

    // Phase 4: Prefix scans (hot path for graph traversal)
    println!("Phase 4: Prefix scans (graph workload)");
    let num_scans = 1000;
    let start = Instant::now();

    for i in 0..num_scans {
        let prefix = format!("key_{:04}", i);
        let mut count = 0;
        let iter = db.prefix(prefix.as_bytes()).unwrap();
        for result in iter {
            let _entry = result.unwrap();
            count += 1;
        }
        // Each scan should return ~10 keys
        assert!(count > 0, "Prefix scan returned no results");
    }

    let duration = start.elapsed();
    println!("  Scan time: {:.2}s", duration.as_secs_f64());
    println!(
        "  Scan throughput: {:.0} scans/sec",
        num_scans as f64 / duration.as_secs_f64()
    );
    println!(
        "  Avg latency: {:.2} ms/scan",
        duration.as_millis() as f64 / num_scans as f64
    );
    println!();

    // Stats
    let stats = db.stats();
    println!("Database Stats:");
    println!("  Memtable entries: {}", db.memtable_len());
    println!("  Memtable size: {} bytes", stats.memtable_size_bytes);
    println!("  Total SSTables: {}", stats.total_sstables);
    println!("  Cache hits: {}", stats.cache_hits);
    println!("  Cache misses: {}", stats.cache_misses);
    println!(
        "  Cache hit rate: {:.2}%",
        if stats.cache_hits + stats.cache_misses > 0 {
            100.0 * stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64
        } else {
            0.0
        }
    );

    println!("\n=== Profiling Complete ===");
}

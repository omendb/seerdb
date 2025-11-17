// Write amplification benchmark
// Measures how much data is written to disk vs logical data written by user

use seerdb::{DBOptions, SyncPolicy, DB};
use std::path::PathBuf;
use std::time::Instant;
use tempfile::TempDir;

fn get_dir_size(path: &std::path::Path) -> u64 {
    let mut size = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    size += metadata.len();
                } else if metadata.is_dir() {
                    size += get_dir_size(&entry.path());
                }
            }
        }
    }
    size
}

fn main() {
    println!("=== Write Amplification Benchmark ===\n");

    let operations = 500_000;
    let value_size = 1024; // 1KB
    let logical_bytes = operations * value_size;

    println!("Operations: {}", operations);
    println!("Value size: {} bytes", value_size);
    println!("Logical data: {} MB\n", logical_bytes / 1024 / 1024);

    let temp_dir = TempDir::new().unwrap();
    let data_dir = PathBuf::from(temp_dir.path());

    let opts = DBOptions {
        data_dir: data_dir.clone(),
        memtable_capacity: 64 * 1024 * 1024, // 64MB memtable
        background_compaction: true,         // Enable compaction
        wal_sync_policy: SyncPolicy::None,   // Fast mode for benchmarking
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();

    println!("Writing {} operations...", operations);
    let start = Instant::now();

    for i in 0..operations {
        let key = format!("key_{:08}", i);
        let value = vec![b'x'; value_size];
        db.put(key.as_bytes(), &value).unwrap();

        if i % 100_000 == 0 && i > 0 {
            println!("  {} ops written", i);
        }
    }

    // Flush to ensure all data on disk
    db.flush().unwrap();
    println!("  Flush complete");

    // Wait for background compaction
    println!("  Waiting for compaction...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    let elapsed = start.elapsed();
    println!("  Total time: {:.2}s\n", elapsed.as_secs_f64());

    // Measure physical bytes written
    let physical_bytes = get_dir_size(&data_dir);

    println!("Results:");
    println!("  Logical data:  {} MB", logical_bytes / 1024 / 1024);
    println!("  Physical data: {} MB", physical_bytes / 1024 / 1024);
    println!(
        "  Write amplification: {:.2}x",
        physical_bytes as f64 / logical_bytes as f64
    );
    println!();

    println!("Comparison:");
    println!("  RocksDB typical:  10-30x write amplification");
    println!("  WiscKey target:   <5x write amplification");
    println!(
        "  seerdb (current): {:.2}x",
        physical_bytes as f64 / logical_bytes as f64
    );
}

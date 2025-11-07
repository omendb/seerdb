// Measure time spent in iteration vs creation
use seerdb::{DBOptions, DB};
use std::path::PathBuf;
use std::time::Instant;

const NUM_OPERATIONS: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    let path = PathBuf::from("/tmp/measure_iteration");
    let _ = std::fs::remove_dir_all(&path);

    let opts = DBOptions {
        data_dir: path.clone(),
        memtable_capacity: 64 * 1024 * 1024,
        wal_sync_policy: seerdb::SyncPolicy::None,
        background_compaction: true,
        vlog_threshold: Some(4096),
        ..Default::default()
    };

    let db = DB::open(opts).expect("Failed to open seerdb");
    let value = vec![0u8; VALUE_SIZE];

    // Write 100K
    for i in 0..NUM_OPERATIONS {
        let key = format!("key_{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }

    // Read 100K
    for i in 0..NUM_OPERATIONS {
        let key = format!("key_{:08}", i);
        let _ = db.get(key.as_bytes()).unwrap();
    }

    // Mixed 100K
    for i in 0..NUM_OPERATIONS {
        if i % 2 == 0 {
            let key = format!("key_{:08}", i + NUM_OPERATIONS);
            db.put(key.as_bytes(), &value).unwrap();
        } else {
            let key = format!("key_{:08}", i);
            let _ = db.get(key.as_bytes()).unwrap();
        }
    }

    println!("=== Measuring Iteration Time ===\n");

    // Measure 100 scans in detail
    let mut total_create_time = std::time::Duration::ZERO;
    let mut total_iterate_time = std::time::Duration::ZERO;
    let mut total_entries = 0;

    for i in 0..100 {
        let start_key = format!("key_{:08}", i * 100);
        let end_key = format!("key_{:08}", i * 100 + 100);

        // Measure creation
        let create_start = Instant::now();
        let iter = db.range(start_key.as_bytes(), Some(end_key.as_bytes())).unwrap();
        let create_time = create_start.elapsed();
        total_create_time += create_time;

        // Measure iteration
        let iterate_start = Instant::now();
        let mut count = 0;
        for result in iter {
            let _ = result.unwrap();
            count += 1;
            if count >= 100 {
                break;
            }
        }
        let iterate_time = iterate_start.elapsed();
        total_iterate_time += iterate_time;
        total_entries += count;
    }

    let avg_create_us = total_create_time.as_micros() / 100;
    let avg_iterate_us = total_iterate_time.as_micros() / 100;
    let total_per_scan_us = (total_create_time + total_iterate_time).as_micros() / 100;

    println!("100 scans, ~100 keys each:");
    println!("  Total entries: {}", total_entries);
    println!("  Avg create time: {} µs", avg_create_us);
    println!("  Avg iterate time: {} µs", avg_iterate_us);
    println!("  Total per scan: {} µs", total_per_scan_us);
    println!("  Iterate/create ratio: {:.1}x", avg_iterate_us as f64 / avg_create_us as f64);
    println!("\nTime per entry during iteration: {:.2} µs",
        total_iterate_time.as_micros() as f64 / total_entries as f64);
}

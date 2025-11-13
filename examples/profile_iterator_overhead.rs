// Profile to understand where remaining overhead is
use seerdb::{DB, DBOptions};
use std::path::PathBuf;
use std::time::Instant;

const NUM_OPERATIONS: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    let path = PathBuf::from("/tmp/profile_iter_overhead");
    let _ = std::fs::remove_dir_all(&path);

    let opts = DBOptions {
        data_dir: path.clone(),
        memtable_capacity: 64 * 1024 * 1024,
        wal_sync_policy: seerdb::SyncPolicy::None,
        background_compaction: true,
        vlog_threshold: Some(4096),
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();
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

    println!("=== Iterator Overhead Analysis ===\n");

    // Test 1: Create iterator, don't iterate
    println!("1. Just creating iterators (no iteration):");
    let start = Instant::now();
    for i in 0..10000 {
        let start_key = format!("key_{:08}", i * 10);
        let end_key = format!("key_{:08}", i * 10 + 100);
        let _ = db
            .range(start_key.as_bytes(), Some(end_key.as_bytes()))
            .unwrap();
        // Don't iterate, just drop
    }
    let elapsed = start.elapsed();
    println!("   10K iterator creations: {:.3}s", elapsed.as_secs_f64());
    println!(
        "   Per iterator: {:.2} µs\n",
        elapsed.as_micros() as f64 / 10000.0
    );

    // Test 2: Create and consume first entry
    println!("2. Create + get first entry:");
    let start = Instant::now();
    for i in 0..10000 {
        let start_key = format!("key_{:08}", i * 10);
        let end_key = format!("key_{:08}", i * 10 + 100);
        let mut iter = db
            .range(start_key.as_bytes(), Some(end_key.as_bytes()))
            .unwrap();
        let _ = iter.next(); // Just first entry
    }
    let elapsed = start.elapsed();
    println!("   10K scans (first entry): {:.3}s", elapsed.as_secs_f64());
    println!(
        "   Per scan: {:.2} µs\n",
        elapsed.as_micros() as f64 / 10000.0
    );

    // Test 3: Create and consume all entries (10 per scan)
    println!("3. Create + iterate 10 entries:");
    let start = Instant::now();
    let mut total = 0;
    for i in 0..10000 {
        let start_key = format!("key_{:08}", i * 10);
        let end_key = format!("key_{:08}", i * 10 + 10);
        for result in db
            .range(start_key.as_bytes(), Some(end_key.as_bytes()))
            .unwrap()
        {
            let _ = result.unwrap();
            total += 1;
        }
    }
    let elapsed = start.elapsed();
    println!(
        "   10K scans (10 entries each): {:.3}s",
        elapsed.as_secs_f64()
    );
    println!("   Total entries: {}", total);
    println!(
        "   Per scan: {:.2} µs",
        elapsed.as_micros() as f64 / 10000.0
    );
    println!(
        "   Per entry: {:.2} µs\n",
        elapsed.as_micros() as f64 / total as f64
    );

    // Test 4: Create and consume all entries (100 per scan)
    println!("4. Create + iterate 100 entries:");
    let start = Instant::now();
    let mut total = 0;
    for i in 0..1000 {
        let start_key = format!("key_{:08}", i * 100);
        let end_key = format!("key_{:08}", i * 100 + 100);
        let mut count = 0;
        for result in db
            .range(start_key.as_bytes(), Some(end_key.as_bytes()))
            .unwrap()
        {
            let _ = result.unwrap();
            count += 1;
            if count >= 100 {
                break;
            }
        }
        total += count;
    }
    let elapsed = start.elapsed();
    println!(
        "   1K scans (100 entries each): {:.3}s",
        elapsed.as_secs_f64()
    );
    println!("   Scans/sec: {:.0}", 1000.0 / elapsed.as_secs_f64());
    println!("   Total entries: {}", total);
    println!("   Per scan: {:.2} µs", elapsed.as_micros() as f64 / 1000.0);
    println!(
        "   Per entry: {:.2} µs\n",
        elapsed.as_micros() as f64 / total as f64
    );

    println!("=== Analysis ===");
    println!("If 'per entry' cost is high (>1µs):");
    println!("  → Iterator overhead (trait objects, boxing, error handling)");
    println!("If 'create' cost is high but 'per entry' is low:");
    println!("  → Setup overhead (cache access, k-way merge initialization)");
    println!("RocksDB reference: ~20,633 scans/sec = ~48µs per 100-entry scan");
    println!("Our current: ~16,644 scans/sec = ~60µs per 100-entry scan");
    println!("Gap: ~12µs per scan (20% slower)");
}

// Match baseline_benchmark sequence exactly to profile range scan bottleneck
use seerdb::{DB, DBOptions};
use std::path::PathBuf;
use std::time::Instant;

const NUM_OPERATIONS: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    let path = PathBuf::from("/tmp/profile_baseline_seq");
    let _ = std::fs::remove_dir_all(&path);

    let opts = DBOptions {
        data_dir: path.clone(),
        memtable_capacity: 64 * 1024 * 1024, // 64MB memtable (same as baseline)
        wal_sync_policy: seerdb::SyncPolicy::None, // Fast benchmark mode
        background_compaction: true,
        vlog_threshold: Some(4096), // Enable vLog for values >4KB
        ..Default::default()
    };

    let db = DB::open(opts).expect("Failed to open seerdb");
    let value = vec![0u8; VALUE_SIZE];

    // Workload 1: Sequential Writes (100K)
    println!("Workload 1: Sequential Writes ({} ops)", NUM_OPERATIONS);
    let start = Instant::now();
    for i in 0..NUM_OPERATIONS {
        let key = format!("key_{:08}", i);
        db.put(key.as_bytes(), &value).expect("Put failed");
    }
    println!("  Time: {:.2}s\n", start.elapsed().as_secs_f64());

    // Workload 2: Random Reads (100K)
    println!("Workload 2: Random Reads ({} ops)", NUM_OPERATIONS);
    let start = Instant::now();
    for i in 0..NUM_OPERATIONS {
        let key = format!("key_{:08}", i);
        let _ = db.get(key.as_bytes()).expect("Get failed");
    }
    println!("  Time: {:.2}s\n", start.elapsed().as_secs_f64());

    // Workload 3: Mixed (50K more writes + 50K reads)
    println!("Workload 3: Mixed 50/50 ({} ops)", NUM_OPERATIONS);
    let start = Instant::now();
    for i in 0..NUM_OPERATIONS {
        if i % 2 == 0 {
            // Write (adds 50K more entries: key_00100000 to key_00149999)
            let key = format!("key_{:08}", i + NUM_OPERATIONS);
            db.put(key.as_bytes(), &value).expect("Put failed");
        } else {
            // Read
            let key = format!("key_{:08}", i);
            let _ = db.get(key.as_bytes()).expect("Get failed");
        }
    }
    println!("  Time: {:.2}s\n", start.elapsed().as_secs_f64());
    println!("Total entries in DB: ~150K (100K initial + 50K from mixed)\n");

    // Workload 4: Range Scans - PROFILE THIS
    println!("=== Profiling Range Scans ===\n");

    // Test 1: Baseline (like baseline_benchmark)
    println!("1. Baseline (1000 scans, 100 keys each):");
    let start = Instant::now();
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
    }
    let elapsed = start.elapsed();
    println!("   Time: {:.3}s", elapsed.as_secs_f64());
    println!("   Scans/sec: {:.0}", 1000.0 / elapsed.as_secs_f64());
    println!(
        "   Time per scan: {:.2} ms\n",
        elapsed.as_millis() as f64 / 1000.0
    );

    // Test 2: After explicit flush
    println!("2. After explicit flush:");
    db.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200)); // Let compaction settle

    let start = Instant::now();
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
    }
    let elapsed = start.elapsed();
    println!("   Time: {:.3}s", elapsed.as_secs_f64());
    println!("   Scans/sec: {:.0}", 1000.0 / elapsed.as_secs_f64());
    println!(
        "   Time per scan: {:.2} ms\n",
        elapsed.as_millis() as f64 / 1000.0
    );

    // Test 3: Single large scan to check overall iterator performance
    println!("3. Single large scan (10K entries):");
    let start = Instant::now();
    let start_key = format!("key_{:08}", 0);
    let end_key = format!("key_{:08}", 10_000);
    let count = db
        .range(start_key.as_bytes(), Some(end_key.as_bytes()))
        .unwrap()
        .count();
    let elapsed = start.elapsed();
    println!("   Returned {} entries", count);
    println!("   Time: {:.3}s", elapsed.as_secs_f64());
    println!(
        "   Time per entry: {:.2} µs\n",
        elapsed.as_micros() as f64 / count as f64
    );

    println!("=== Analysis ===");
    println!("If scans are slow (~877/sec):");
    println!("  → Problem reproduced, now we can profile");
    println!("If scans are fast (>5000/sec):");
    println!("  → Cannot reproduce, issue may be timing/state dependent");
}

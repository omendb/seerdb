// Profile range scan to identify bottleneck (memtable vs SSTable iteration)
use seerdb::{DB, DBOptions};
use std::time::Instant;
use tempfile::TempDir;

fn main() {
    let temp_dir = TempDir::new().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = temp_dir.path().to_path_buf();
    opts.background_compaction = true;
    opts.memtable_capacity = 64 * 1024 * 1024; // 64MB like baseline_benchmark

    let db = DB::open(opts).unwrap();

    // Write 100K entries like baseline_benchmark
    println!("Writing 100K entries...");
    let value = vec![0u8; 1024];
    let start = Instant::now();
    for i in 0..100_000 {
        let key = format!("key_{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }
    println!("Write time: {:.2}s", start.elapsed().as_secs_f64());

    // Check how many SSTables were created
    println!("\nChecking LSM state...");
    // Note: We'd need to expose LSM stats, for now just run scans

    println!("\n=== Profiling Range Scans ===\n");

    // Profile 1: Single large scan (100K entries)
    println!("1. Single large scan (100K entries):");
    let start = Instant::now();
    let start_key = format!("key_{:08}", 0);
    let end_key = format!("key_{:08}", 100_000);
    let count = db
        .range(start_key.as_bytes(), Some(end_key.as_bytes()))
        .unwrap()
        .count();
    let elapsed = start.elapsed();
    println!("   Returned {} entries", count);
    println!("   Time: {:.3}s", elapsed.as_secs_f64());
    println!(
        "   Time per entry: {:.2} µs",
        elapsed.as_micros() as f64 / count as f64
    );

    // Profile 2: Many small scans (1000 scans, 100 keys each) - like baseline_benchmark
    println!("\n2. Many small scans (1000 scans, 100 keys each):");
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
        "   Time per scan: {:.2} ms",
        elapsed.as_millis() as f64 / 1000.0
    );

    // Profile 3: Check memtable size by looking at first few entries
    println!("\n3. Quick memtable iteration test (first 1000 keys):");
    let start = Instant::now();
    let start_key = format!("key_{:08}", 0);
    let end_key = format!("key_{:08}", 1000);
    let count = db
        .range(start_key.as_bytes(), Some(end_key.as_bytes()))
        .unwrap()
        .count();
    let elapsed = start.elapsed();
    println!("   Returned {} entries", count);
    println!("   Time: {:.3} ms", elapsed.as_millis() as f64);

    // Profile 4: Force flush and try again
    println!("\n4. After flush:");
    db.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100)); // Let flush complete

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
        "   Time per scan: {:.2} ms",
        elapsed.as_millis() as f64 / 1000.0
    );

    println!("\n=== Summary ===");
    println!("If scans are slow both before and after flush:");
    println!("  → Likely SSTable iteration or k-way merge overhead");
    println!("If scans are fast after flush:");
    println!("  → Memtable collection is the bottleneck");
}

// ALEX vs partition_point benchmark
// Debug why ALEX lower_bound is slower than partition_point

use seerdb::{DBOptions, DB};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("ALEX Debug Benchmark");
    println!("====================\n");

    let temp_dir = std::env::temp_dir().join(format!("alex_debug_{}", std::process::id()));
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }

    // Create DB with default options (ALEX disabled)
    let mut options = DBOptions::default();
    options.data_dir = temp_dir.clone();
    let db = DB::open(options)?;

    // Write enough data to create SSTables with index blocks
    println!("Writing 50K keys to create SSTables...");
    for i in 0..50_000 {
        let key = format!("key{:08}", i);
        let value = vec![b'x'; 1024]; // 1KB
        db.put(key.as_bytes(), &value)?;
    }

    // Force flush to create SSTables
    drop(db);

    // Reopen and benchmark reads
    let mut options = DBOptions::default();
    options.data_dir = temp_dir.clone();
    let db = DB::open(options)?;

    println!("\nBenchmarking random reads (100K ops)...\n");

    let start = Instant::now();
    for i in 0..100_000 {
        let key = format!("key{:08}", i % 50_000);
        let _ = db.get(key.as_bytes())?;
    }
    let elapsed = start.elapsed();
    let ops_per_sec = 100_000.0 / elapsed.as_secs_f64();

    println!("Results:");
    println!("  Total: {:.2}s", elapsed.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", ops_per_sec);
    println!(
        "  Latency: {:.2} µs/op",
        elapsed.as_micros() as f64 / 100_000.0
    );

    // Cleanup
    drop(db);
    std::fs::remove_dir_all(&temp_dir)?;

    println!("\nNow run with ALEX enabled to compare...");

    Ok(())
}

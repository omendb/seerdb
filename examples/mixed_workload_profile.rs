// Mixed workload profiling benchmark
// Run with: cargo flamegraph --example mixed_workload_profile --release
//
// This benchmark focuses on the mixed 50/50 read/write workload to identify
// bottlenecks preventing us from reaching fjall's performance (577K ops/sec)
//
// Current: 415K ops/sec seerdb vs 577K fjall (-28%)
// Goal: Identify hot paths and optimization opportunities

use seerdb::{DBOptions, DB};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = std::env::temp_dir().join(format!("seerdb_mixed_profile_{}", std::process::id()));

    // Clean start
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }

    let mut options = DBOptions::default();
    options.data_dir = temp_dir.clone();
    options.memtable_capacity = 256 * 1024 * 1024; // 256MB memtable

    let db = DB::open(options)?;

    println!("Starting mixed workload profiling...");
    println!("This will run for ~60 seconds to generate a good flame graph");

    // Pre-populate with 50K keys for mixed read/write
    println!("\nPre-populating 50K keys...");
    for i in 0..50_000 {
        let key = format!("key{:08}", i);
        let value = vec![b'x'; 1024]; // 1KB value
        db.put(key.as_bytes(), &value)?;
    }

    println!("Pre-population complete. Starting mixed workload...");

    // Mixed workload: 50% reads, 50% writes
    // Run for longer to get good profiling data
    let total_ops = 500_000;
    let start = Instant::now();

    for i in 0..total_ops {
        if i % 2 == 0 {
            // Write (50%)
            let key = format!("key{:08}", i % 100_000);
            let value = vec![b'x'; 1024];
            db.put(key.as_bytes(), &value)?;
        } else {
            // Read (50%)
            let key = format!("key{:08}", i % 50_000);
            let _ = db.get(key.as_bytes())?;
        }

        // Progress indicator
        if i > 0 && i % 50_000 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let ops_per_sec = i as f64 / elapsed;
            println!("Progress: {} ops, {:.0} ops/sec", i, ops_per_sec);
        }
    }

    let elapsed = start.elapsed();
    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();

    println!("\nMixed workload complete!");
    println!("Total operations: {}", total_ops);
    println!("Time: {:.2}s", elapsed.as_secs_f64());
    println!("Throughput: {:.0} ops/sec", ops_per_sec);
    println!("\nCheck the flame graph to identify hot paths.");

    // Cleanup
    drop(db);
    std::fs::remove_dir_all(&temp_dir)?;

    Ok(())
}

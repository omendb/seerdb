//! Profile mixed workload to identify bottleneck
//!
//! Run with: cargo flamegraph --release --example profile_mixed_workload

use seerdb::{DBOptions, DB};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;

    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 64 * 1024 * 1024, // 64MB
        ..Default::default()
    };

    let db = DB::open(options)?;

    println!("=== Mixed Workload Profile (500K ops) ===");
    println!("50% writes, 50% reads");
    println!();

    let start = Instant::now();

    // Mixed workload: 50% writes, 50% reads
    for i in 0..500_000 {
        if i % 2 == 0 {
            // Write
            let key = format!("key_{:08}", i);
            let value = vec![0u8; 1024];
            db.put(key.as_bytes(), &value)?;
        } else {
            // Read
            let key = format!("key_{:08}", i - 1);
            let _ = db.get(key.as_bytes())?;
        }

        // Print progress every 100K ops
        if i > 0 && i % 100_000 == 0 {
            let elapsed = start.elapsed();
            let throughput = i as f64 / elapsed.as_secs_f64();
            println!("{} ops: {:.0} ops/sec", i, throughput);
        }
    }

    let elapsed = start.elapsed();
    let throughput = 500_000.0 / elapsed.as_secs_f64();

    println!();
    println!("Final throughput: {:.0} ops/sec", throughput);
    println!("Total time: {:.2}s", elapsed.as_secs_f64());

    // Keep DB alive for profiling
    drop(db);

    Ok(())
}

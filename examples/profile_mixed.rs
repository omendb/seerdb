//! Profile mixed workload to identify bottlenecks

use seerdb::{DB, DBOptions};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;

    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 64 * 1024 * 1024, // 64MB
        ..Default::default()
    };

    let db = DB::open(options)?;

    println!("Starting mixed workload profile (500K ops)...");
    let start = Instant::now();

    // Mixed 50/50 read/write workload
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

        if i % 100_000 == 0 {
            println!("Progress: {}/500000", i);
        }
    }

    let elapsed = start.elapsed();
    let throughput = 500_000.0 / elapsed.as_secs_f64();

    println!("\nCompleted!");
    println!("Time: {:.2}s", elapsed.as_secs_f64());
    println!("Throughput: {:.0} ops/sec", throughput);
    println!("\nProfile with: cargo run --release --example profile_mixed");
    println!("Then analyze with cargo-flamegraph or samply");

    Ok(())
}

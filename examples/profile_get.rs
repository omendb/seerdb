// Profile point query performance
use seerdb::{DB, DBOptions};
use std::time::Instant;
use tempfile::TempDir;

fn main() {
    let temp_dir = TempDir::new().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = temp_dir.path().to_path_buf();
    opts.background_compaction = false;

    let db = DB::open(opts).unwrap();

    // Write 100K entries
    println!("Writing 100K entries...");
    for i in 0..100_000 {
        let key = format!("key{:08}", i);
        let value = "x".repeat(100);
        db.put(key.as_bytes(), value.as_bytes()).unwrap();
    }

    db.flush().unwrap();
    println!("Flushed to SSTables");

    // Warmup
    for i in 0..1000 {
        let key = format!("key{:08}", i);
        let _ = db.get(key.as_bytes());
    }

    // Benchmark point queries
    let num_ops = 100_000;
    println!("\nBenchmarking {} point queries...", num_ops);

    let start = Instant::now();
    for i in 0..num_ops {
        let key = format!("key{:08}", i);
        let _ = db.get(key.as_bytes()).unwrap();
    }
    let elapsed = start.elapsed();

    let ops_per_sec = num_ops as f64 / elapsed.as_secs_f64();
    let us_per_op = elapsed.as_micros() as f64 / num_ops as f64;

    println!("\nResults:");
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());
    println!("  Ops/sec: {:.0}", ops_per_sec);
    println!("  Time per op: {:.2} µs", us_per_op);
}

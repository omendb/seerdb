// Simple write benchmark
use seerdb::{DBOptions, SyncPolicy, DB};
use std::time::Instant;
use tempfile::TempDir;

fn main() {
    let temp_dir = TempDir::new().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = temp_dir.path().to_path_buf();
    opts.background_compaction = false;
    opts.wal_sync_policy = SyncPolicy::None; // Disable fsync for fair throughput comparison

    let db = DB::open(opts).unwrap();

    // Benchmark sequential writes (100K operations, 1KB values like RocksDB baseline)
    let num_ops = 100_000;
    let value = vec![0u8; 1024];

    println!("Benchmarking {} sequential writes (1KB values)...", num_ops);

    let start = Instant::now();
    for i in 0..num_ops {
        let key = format!("key_{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }
    let elapsed = start.elapsed();

    let ops_per_sec = num_ops as f64 / elapsed.as_secs_f64();
    let us_per_op = elapsed.as_micros() as f64 / num_ops as f64;

    println!("\nResults:");
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());
    println!("  Ops/sec: {:.0}", ops_per_sec);
    println!("  Time per op: {:.2} µs", us_per_op);

    // Compare to baseline (RocksDB: 157,616 ops/sec)
    let rocksdb_baseline = 157_616.0;
    let ratio = ops_per_sec / rocksdb_baseline;
    println!(
        "  vs RocksDB baseline: {:.2}x ({:.0}% {})",
        ratio,
        ((1.0 - ratio).abs() * 100.0),
        if ratio >= 1.0 { "faster" } else { "slower" }
    );
}

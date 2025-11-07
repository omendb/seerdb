// Profile flush overhead
// Measure how much time is spent in flush() calls

use seerdb::{DBOptions, DB};
use std::time::Instant;
use tempfile::tempdir;

fn main() {
    println!("=== Flush Overhead Profiling ===\n");

    // Test 1: Small memtable (many flushes)
    println!("Test 1: Small memtable (4MB) - expect many flushes");
    profile_with_flushes(4 * 1024 * 1024, 50_000);

    println!("\nTest 2: Medium memtable (16MB) - expect some flushes");
    profile_with_flushes(16 * 1024 * 1024, 50_000);

    println!("\nTest 3: Large memtable (64MB) - expect few flushes");
    profile_with_flushes(64 * 1024 * 1024, 50_000);
}

fn profile_with_flushes(memtable_capacity: usize, num_ops: usize) {
    let dir = tempdir().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = dir.path().to_path_buf();
    opts.memtable_capacity = memtable_capacity;
    opts.wal_sync_policy = seerdb::SyncPolicy::None; // Isolate flush overhead
    opts.background_compaction = false;
    opts.background_flush = false; // Synchronous flush to measure overhead

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; 1024];

    let mut total_write_time = std::time::Duration::ZERO;
    let mut flush_count = 0;

    let overall_start = Instant::now();

    for i in 0..num_ops {
        let key = format!("key{:08}", i);

        let write_start = Instant::now();
        db.put(key.as_bytes(), &value).unwrap();
        total_write_time += write_start.elapsed();

        // Check if memtable size increased (to detect flushes)
        if i > 0 && i % 5000 == 0 {
            let memtable_size = db.memtable_size();
            if memtable_size < memtable_capacity / 4 {
                flush_count += 1;
            }
        }
    }

    let overall_duration = overall_start.elapsed();

    // Force final flush to measure it
    let flush_start = Instant::now();
    db.flush().unwrap();
    let final_flush_time = flush_start.elapsed();

    let throughput = num_ops as f64 / overall_duration.as_secs_f64();
    let avg_write_latency = total_write_time.as_micros() as f64 / num_ops as f64;
    let flush_overhead = (overall_duration - total_write_time).as_secs_f64();
    let flush_overhead_pct = (flush_overhead / overall_duration.as_secs_f64()) * 100.0;

    println!("  Memtable capacity: {:.1} MB", memtable_capacity as f64 / 1_000_000.0);
    println!("  Operations: {}", num_ops);
    println!("  Total time: {:.3}s", overall_duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", throughput);
    println!("  Avg write latency: {:.2} µs", avg_write_latency);
    println!("  Flush overhead: {:.3}s ({:.1}%)", flush_overhead, flush_overhead_pct);
    println!("  Estimated flushes: ~{}", flush_count);
    println!("  Final flush time: {:.3}s", final_flush_time.as_secs_f64());
}

// Multi-threaded write benchmark to measure partitioned memtable benefits
// Expected: +25-40% throughput due to reduced lock contention

use seerdb::{DB, DBOptions};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use tempfile::tempdir;

const NUM_THREADS: usize = 8;
const WRITES_PER_THREAD: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    println!("=== Multi-threaded Write Benchmark ===");
    println!("Threads: {}", NUM_THREADS);
    println!("Writes per thread: {}", WRITES_PER_THREAD);
    println!("Total writes: {}", NUM_THREADS * WRITES_PER_THREAD);
    println!("Value size: {} bytes", VALUE_SIZE);
    println!();

    // Create temporary directory
    let dir = tempdir().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = dir.path().to_path_buf();
    opts.memtable_capacity = 2048 * 1024 * 1024; // 2GB - avoid flushes during benchmark
    opts.background_flush = false; // Disable to measure pure write throughput
    opts.background_compaction = false;
    opts.wal_sync_policy = seerdb::SyncPolicy::None; // Disable fsync to isolate lock contention

    let db = Arc::new(DB::open(opts).unwrap());

    println!("Running multi-threaded write benchmark...");
    let start = Instant::now();

    // Spawn writer threads
    let mut handles = vec![];
    for thread_id in 0..NUM_THREADS {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            let value = vec![0u8; VALUE_SIZE];
            for i in 0..WRITES_PER_THREAD {
                // Create unique keys per thread to avoid conflicts
                let key = format!("thread{:02}_key{:08}", thread_id, i);
                db_clone.put(key.as_bytes(), &value).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let total_ops = NUM_THREADS * WRITES_PER_THREAD;
    let throughput = total_ops as f64 / duration.as_secs_f64();

    println!();
    println!("=== Results ===");
    println!("Time: {:.2}s", duration.as_secs_f64());
    println!("Throughput: {:.0} ops/sec", throughput);
    println!(
        "Latency: {:.2} us/op",
        duration.as_micros() as f64 / total_ops as f64
    );
    println!();

    println!("Theoretical improvement vs single-threaded:");
    println!("- Single-threaded baseline: ~218K ops/sec");
    println!(
        "- Multi-threaded with {} threads: {:.0} ops/sec",
        NUM_THREADS, throughput
    );
    println!("- Speedup: {:.2}x", throughput / 218_000.0);
    println!();

    if throughput > 270_000.0 {
        println!("✅ SUCCESS: Achieved >270K ops/sec (+25% improvement)");
    } else {
        println!("⚠️  Below target of 270K ops/sec (+25%)");
    }
}

// Lock contention profiling benchmark
//
// Tests concurrent write/read performance to identify lock bottlenecks
// Run with: cargo run --release --example lock_contention_benchmark

use seerdb::{DBOptions, DB};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

const THREADS: usize = 16; // High thread count to stress locks
const OPS_PER_THREAD: usize = 10_000;
const VALUE_SIZE: usize = 1024; // 1KB values

fn main() {
    println!("=== Lock Contention Profiling ===\n");
    println!("Threads: {}", THREADS);
    println!("Ops per thread: {}", OPS_PER_THREAD);
    println!("Total ops: {}\n", THREADS * OPS_PER_THREAD);

    // Test 1: Concurrent writes
    println!("Test 1: Concurrent Writes (high contention)");
    run_concurrent_writes();
    println!();

    // Test 2: Concurrent reads
    println!("Test 2: Concurrent Reads (lock-free cache)");
    run_concurrent_reads();
    println!();

    // Test 3: Mixed workload
    println!("Test 3: Mixed Workload (50% read, 50% write)");
    run_mixed_workload();
    println!();

    // Test 4: Batch writes
    println!("Test 4: Concurrent Batch Writes");
    run_concurrent_batches();
    println!();

    println!("=== Lock Contention Profiling Complete ===");
    println!("\nAnalysis:");
    println!("- If concurrent write throughput << single-threaded × threads:");
    println!("  → Lock contention on memtable or WAL");
    println!("- If concurrent read throughput scales linearly:");
    println!("  → Lock-free cache is working (expected)");
    println!("- Check latency percentiles for blocking (high p99 = contention)");
}

fn run_concurrent_writes() {
    let dir = tempdir().unwrap();
    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 64 * 1024 * 1024, // Large to avoid flushes
        block_cache_capacity: 16_384,
        ..Default::default()
    };

    let db = Arc::new(DB::open(options).unwrap());
    let total_ops = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let handles: Vec<_> = (0..THREADS)
        .map(|thread_id| {
            let db = db.clone();
            let total_ops = total_ops.clone();

            thread::spawn(move || {
                let value = vec![b'x'; VALUE_SIZE];
                let thread_start = Instant::now();

                for i in 0..OPS_PER_THREAD {
                    let key = format!("thread:{}:key:{:08}", thread_id, i);
                    db.put(key.as_bytes(), &value).unwrap();
                    total_ops.fetch_add(1, Ordering::Relaxed);
                }

                let elapsed = thread_start.elapsed();
                let throughput = OPS_PER_THREAD as f64 / elapsed.as_secs_f64();
                (elapsed, throughput)
            })
        })
        .collect();

    let mut thread_results = Vec::new();
    for handle in handles {
        thread_results.push(handle.join().unwrap());
    }

    let total_elapsed = start.elapsed();
    let total = total_ops.load(Ordering::Relaxed);

    // Analysis
    let total_throughput = total as f64 / total_elapsed.as_secs_f64();
    let avg_thread_throughput: f64 = thread_results.iter().map(|(_, t)| t).sum::<f64>() / THREADS as f64;
    let max_thread_time = thread_results.iter().map(|(t, _)| t).max().unwrap();
    let min_thread_time = thread_results.iter().map(|(t, _)| t).min().unwrap();

    println!("  Total time: {:?}", total_elapsed);
    println!("  Total throughput: {:.0} writes/sec", total_throughput);
    println!("  Avg thread throughput: {:.0} writes/sec", avg_thread_throughput);
    println!("  Thread time range: {:?} - {:?}", min_thread_time, max_thread_time);
    println!("  Time variance: {:?}", max_thread_time.as_secs_f64() / min_thread_time.as_secs_f64());

    let stats = db.stats();
    println!("  Memtable size: {:.2} MB", stats.memtable_size_bytes as f64 / 1024.0 / 1024.0);
    println!("  Write amp: {:.2}x", stats.write_amplification);

    // Contention indicator
    let ideal_throughput = avg_thread_throughput * THREADS as f64;
    let efficiency = (total_throughput / ideal_throughput) * 100.0;
    println!("  Parallel efficiency: {:.1}%", efficiency);
    if efficiency < 70.0 {
        println!("  ⚠️  Low efficiency suggests lock contention");
    } else {
        println!("  ✅ Good parallel scaling");
    }
}

fn run_concurrent_reads() {
    let dir = tempdir().unwrap();
    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 64 * 1024 * 1024,
        block_cache_capacity: 16_384,
        ..Default::default()
    };

    let db = Arc::new(DB::open(options).unwrap());

    // Setup: Write data first
    println!("  Setup: Writing {} keys...", OPS_PER_THREAD);
    let value = vec![b'x'; VALUE_SIZE];
    for i in 0..OPS_PER_THREAD {
        let key = format!("key:{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }
    db.flush().unwrap();
    println!("  Setup complete");

    let total_ops = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let handles: Vec<_> = (0..THREADS)
        .map(|_thread_id| {
            let db = db.clone();
            let total_ops = total_ops.clone();

            thread::spawn(move || {
                let thread_start = Instant::now();
                let mut hits = 0;

                for i in 0..OPS_PER_THREAD {
                    let key = format!("key:{:08}", i % OPS_PER_THREAD);
                    if db.get(key.as_bytes()).unwrap().is_some() {
                        hits += 1;
                    }
                    total_ops.fetch_add(1, Ordering::Relaxed);
                }

                let elapsed = thread_start.elapsed();
                let throughput = OPS_PER_THREAD as f64 / elapsed.as_secs_f64();
                (elapsed, throughput, hits)
            })
        })
        .collect();

    let mut thread_results = Vec::new();
    for handle in handles {
        thread_results.push(handle.join().unwrap());
    }

    let total_elapsed = start.elapsed();
    let total = total_ops.load(Ordering::Relaxed);

    // Analysis
    let total_throughput = total as f64 / total_elapsed.as_secs_f64();
    let avg_thread_throughput: f64 = thread_results.iter().map(|(_, t, _)| t).sum::<f64>() / THREADS as f64;

    println!("  Total time: {:?}", total_elapsed);
    println!("  Total throughput: {:.0} reads/sec", total_throughput);
    println!("  Avg thread throughput: {:.0} reads/sec", avg_thread_throughput);

    let stats = db.stats();
    println!("  Cache hit rate: {:.2}%", stats.cache_hit_rate * 100.0);

    // Lock-free cache should scale linearly
    let ideal_throughput = avg_thread_throughput * THREADS as f64;
    let efficiency = (total_throughput / ideal_throughput) * 100.0;
    println!("  Parallel efficiency: {:.1}%", efficiency);
    if efficiency > 90.0 {
        println!("  ✅ Excellent scaling (lock-free cache working)");
    } else if efficiency > 70.0 {
        println!("  ✅ Good scaling");
    } else {
        println!("  ⚠️  Unexpected contention in read path");
    }
}

fn run_mixed_workload() {
    let dir = tempdir().unwrap();
    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 64 * 1024 * 1024,
        block_cache_capacity: 16_384,
        ..Default::default()
    };

    let db = Arc::new(DB::open(options).unwrap());

    // Setup: Write initial data
    let value = vec![b'x'; VALUE_SIZE];
    for i in 0..OPS_PER_THREAD {
        let key = format!("key:{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }
    db.flush().unwrap();

    let total_ops = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let handles: Vec<_> = (0..THREADS)
        .map(|thread_id| {
            let db = db.clone();
            let total_ops = total_ops.clone();

            thread::spawn(move || {
                let value = vec![b'x'; VALUE_SIZE];
                let thread_start = Instant::now();
                let mut reads = 0;
                let mut writes = 0;

                for i in 0..OPS_PER_THREAD {
                    if i % 2 == 0 {
                        // Write
                        let key = format!("thread:{}:key:{:08}", thread_id, i);
                        db.put(key.as_bytes(), &value).unwrap();
                        writes += 1;
                    } else {
                        // Read
                        let key = format!("key:{:08}", i % OPS_PER_THREAD);
                        let _ = db.get(key.as_bytes()).unwrap();
                        reads += 1;
                    }
                    total_ops.fetch_add(1, Ordering::Relaxed);
                }

                let elapsed = thread_start.elapsed();
                let throughput = OPS_PER_THREAD as f64 / elapsed.as_secs_f64();
                (elapsed, throughput, reads, writes)
            })
        })
        .collect();

    let mut thread_results = Vec::new();
    for handle in handles {
        thread_results.push(handle.join().unwrap());
    }

    let total_elapsed = start.elapsed();
    let total = total_ops.load(Ordering::Relaxed);
    let total_reads: usize = thread_results.iter().map(|(_, _, r, _)| r).sum();
    let total_writes: usize = thread_results.iter().map(|(_, _, _, w)| w).sum();

    // Analysis
    let total_throughput = total as f64 / total_elapsed.as_secs_f64();

    println!("  Total time: {:?}", total_elapsed);
    println!("  Total throughput: {:.0} ops/sec", total_throughput);
    println!("  Reads: {}, Writes: {}", total_reads, total_writes);

    let stats = db.stats();
    println!("  Cache hit rate: {:.2}%", stats.cache_hit_rate * 100.0);
    println!("  Memtable size: {:.2} MB", stats.memtable_size_bytes as f64 / 1024.0 / 1024.0);
}

fn run_concurrent_batches() {
    let dir = tempdir().unwrap();
    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 64 * 1024 * 1024,
        block_cache_capacity: 16_384,
        ..Default::default()
    };

    let db = Arc::new(DB::open(options).unwrap());
    let total_ops = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    const BATCH_SIZE: usize = 100;

    let handles: Vec<_> = (0..THREADS)
        .map(|thread_id| {
            let db = db.clone();
            let total_ops = total_ops.clone();

            thread::spawn(move || {
                let value = vec![b'x'; VALUE_SIZE];
                let thread_start = Instant::now();

                for batch_idx in 0..(OPS_PER_THREAD / BATCH_SIZE) {
                    let mut batch = db.batch();

                    for i in 0..BATCH_SIZE {
                        let key = format!("thread:{}:batch:{}:key:{:03}", thread_id, batch_idx, i);
                        batch.put(key.as_bytes(), &value);
                    }

                    batch.commit().unwrap();
                    total_ops.fetch_add(BATCH_SIZE as u64, Ordering::Relaxed);
                }

                let elapsed = thread_start.elapsed();
                let throughput = OPS_PER_THREAD as f64 / elapsed.as_secs_f64();
                (elapsed, throughput)
            })
        })
        .collect();

    let mut thread_results = Vec::new();
    for handle in handles {
        thread_results.push(handle.join().unwrap());
    }

    let total_elapsed = start.elapsed();
    let total = total_ops.load(Ordering::Relaxed);

    // Analysis
    let total_throughput = total as f64 / total_elapsed.as_secs_f64();
    let avg_thread_throughput: f64 = thread_results.iter().map(|(_, t)| t).sum::<f64>() / THREADS as f64;

    println!("  Batch size: {}", BATCH_SIZE);
    println!("  Total time: {:?}", total_elapsed);
    println!("  Total throughput: {:.0} writes/sec", total_throughput);
    println!("  Avg thread throughput: {:.0} writes/sec", avg_thread_throughput);

    let stats = db.stats();
    println!("  Memtable size: {:.2} MB", stats.memtable_size_bytes as f64 / 1024.0 / 1024.0);

    // Batch writes should reduce contention
    let ideal_throughput = avg_thread_throughput * THREADS as f64;
    let efficiency = (total_throughput / ideal_throughput) * 100.0;
    println!("  Parallel efficiency: {:.1}%", efficiency);
}

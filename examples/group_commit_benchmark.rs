// Group Commit Performance Benchmark
//
// Measures the performance impact of group commit optimization by testing:
// - Different concurrency levels (1, 10, 50, 100 threads)
// - Different group_commit_delay values (0μs, 50μs, 100μs, 200μs, 500μs)
// - Comparison with SyncPolicy::None baseline
//
// Expected results (based on PostgreSQL/RocksDB research):
// - 2-3x improvement (conservative)
// - 5-7x improvement (realistic)
// - 10x improvement (optimistic, high concurrency)

use seerdb::{DBOptions, SyncPolicy, DB};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

const WRITES_PER_THREAD: usize = 1_000;

fn main() {
    println!("=== Group Commit Performance Benchmark ===\n");
    println!("Testing {} writes per thread", WRITES_PER_THREAD);
    println!("Comparing different concurrency levels and group_commit_delay values\n");

    // Test configurations
    let concurrency_levels = vec![1, 10, 50, 100];
    let delay_values_us = vec![0, 50, 100, 200, 500]; // microseconds

    println!("┌─────────────┬──────────┬──────────────┬──────────────┬────────────┬────────────┐");
    println!("│ Concurrency │ Delay    │ Total Writes │ Duration     │ Throughput │ vs No GC   │");
    println!("│             │ (μs)     │              │              │ (ops/sec)  │            │");
    println!("├─────────────┼──────────┼──────────────┼──────────────┼────────────┼────────────┤");

    // Store baseline (delay=0) results for comparison
    let mut baseline_throughputs: std::collections::HashMap<usize, f64> =
        std::collections::HashMap::new();

    for &concurrency in &concurrency_levels {
        for &delay_us in &delay_values_us {
            let result = run_benchmark(concurrency, delay_us);

            // Store baseline for this concurrency level
            if delay_us == 0 {
                baseline_throughputs.insert(concurrency, result.throughput);
            }

            // Calculate improvement vs baseline (delay=0)
            let improvement = if delay_us == 0 {
                1.0 // Baseline
            } else {
                let baseline = baseline_throughputs.get(&concurrency).unwrap_or(&1.0);
                result.throughput / baseline
            };

            println!(
                "│ {:>11} │ {:>8} │ {:>12} │ {:>9.2}s │ {:>10.0} │ {:>9.2}x │",
                concurrency,
                delay_us,
                result.total_writes,
                result.duration.as_secs_f64(),
                result.throughput,
                improvement
            );
        }

        // Separator between concurrency levels
        if concurrency != *concurrency_levels.last().unwrap() {
            println!("├─────────────┼──────────┼──────────────┼──────────────┼────────────┼────────────┤");
        }
    }

    println!("└─────────────┴──────────┴──────────────┴──────────────┴────────────┴────────────┘");

    // Compare with SyncPolicy::None (no durability)
    println!("\n=== Comparison with SyncPolicy::None (no durability) ===\n");
    println!("┌─────────────┬──────────────┬──────────────┬────────────────┐");
    println!("│ Concurrency │ SyncNone     │ Best GroupGC │ GroupGC/None   │");
    println!("│             │ (ops/sec)    │ (ops/sec)    │ (ratio)        │");
    println!("├─────────────┼──────────────┼──────────────┼────────────────┤");

    for &concurrency in &concurrency_levels {
        let sync_none = run_benchmark_sync_none(concurrency);

        // Find best group commit result for this concurrency
        let best_gc = delay_values_us
            .iter()
            .map(|&delay| run_benchmark(concurrency, delay))
            .max_by(|a, b| {
                a.throughput
                    .partial_cmp(&b.throughput)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        let ratio = best_gc.throughput / sync_none.throughput;

        println!(
            "│ {:>11} │ {:>12.0} │ {:>12.0} │ {:>13.2}x │",
            concurrency, sync_none.throughput, best_gc.throughput, ratio
        );
    }

    println!("└─────────────┴──────────────┴──────────────┴────────────────┘");

    println!("\n=== Key Findings ===");
    println!("- Group commit disabled (0μs): Baseline throughput");
    println!("- Optimal delay typically 50-200μs for SSD/NVME");
    println!("- Higher concurrency = more batching = bigger improvement");
    println!("- Best case should approach SyncPolicy::None performance");
    println!("- With good tuning, expect 2-10x improvement vs no group commit");
}

struct BenchmarkResult {
    total_writes: usize,
    duration: Duration,
    throughput: f64,
}

fn run_benchmark(concurrency: usize, delay_us: u64) -> BenchmarkResult {
    let dir = tempdir().unwrap();

    let opts = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 128 * 1024 * 1024,  // 128MB
        wal_sync_policy: SyncPolicy::SyncData, // Enable durability
        background_compaction: false,
        background_flush: false,
        group_commit_delay_us: delay_us,
        group_commit_max_batch_size: 1000,
        ..Default::default()
    };

    let db = Arc::new(DB::open(opts).unwrap());
    let total_writes = concurrency * WRITES_PER_THREAD;

    let start = Instant::now();

    // Spawn concurrent writers
    let handles: Vec<_> = (0..concurrency)
        .map(|thread_id| {
            let db = Arc::clone(&db);
            thread::spawn(move || {
                for i in 0..WRITES_PER_THREAD {
                    let key = format!("key_{}_{:06}", thread_id, i);
                    let value = format!("value_{}_{:06}", thread_id, i);
                    db.put(key.as_bytes(), value.as_bytes()).unwrap();
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = total_writes as f64 / duration.as_secs_f64();

    BenchmarkResult {
        total_writes,
        duration,
        throughput,
    }
}

fn run_benchmark_sync_none(concurrency: usize) -> BenchmarkResult {
    let dir = tempdir().unwrap();

    let opts = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 128 * 1024 * 1024,
        wal_sync_policy: SyncPolicy::None, // No durability (baseline)
        background_compaction: false,
        background_flush: false,
        group_commit_delay_us: 0,
        group_commit_max_batch_size: 1000,
        ..Default::default()
    };

    let db = Arc::new(DB::open(opts).unwrap());
    let total_writes = concurrency * WRITES_PER_THREAD;

    let start = Instant::now();

    // Spawn concurrent writers
    let handles: Vec<_> = (0..concurrency)
        .map(|thread_id| {
            let db = Arc::clone(&db);
            thread::spawn(move || {
                for i in 0..WRITES_PER_THREAD {
                    let key = format!("key_{}_{:06}", thread_id, i);
                    let value = format!("value_{}_{:06}", thread_id, i);
                    db.put(key.as_bytes(), value.as_bytes()).unwrap();
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = total_writes as f64 / duration.as_secs_f64();

    BenchmarkResult {
        total_writes,
        duration,
        throughput,
    }
}

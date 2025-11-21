// Stress test to measure write throughput and compaction backpressure
// Run with: cargo run --release --example compaction_stress_test

use rand::Rng;
use seerdb::{DBOptions, SyncPolicy, DB};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn main() {
    let temp_dir = TempDir::new().unwrap();
    println!("Data directory: {:?}", temp_dir.path());

    let mut opts = DBOptions::default();
    opts.data_dir = temp_dir.path().to_path_buf();

    // Enable background workers
    opts.background_compaction = true;
    opts.background_flush = true;

    // Tiny memtable to trigger frequent flushes (1MB total = 64KB per partition)
    opts.memtable_capacity = 1 * 1024 * 1024;

    // Fast WAL sync (Group Commit)
    opts.wal_sync_policy = SyncPolicy::SyncData;
    opts.group_commit_delay_us = 100; // 100us delay
    opts.group_commit_max_batch_size = 4096;

    // Open DB
    let db = Arc::new(DB::open(opts).unwrap());

    println!("Starting compaction stress test...");
    println!("Configuration:");
    println!("  Memtable: 1MB (partitioned)");
    println!("  Background Compaction: true");
    println!("  Background Flush: true");
    println!("  WAL Sync: SyncData (100us group commit)");

    let start_total = Instant::now();
    let num_threads = 8;
    let total_ops = 1_00_000; // 100K writes (reduced for timeout safety)
    let ops_per_thread = total_ops / num_threads;

    let mut handles = Vec::new();

    for t_id in 0..num_threads {
        let db = db.clone();
        let handle = thread::spawn(move || {
            let mut rng = rand::thread_rng();
            let mut value = vec![0u8; 1024]; // 1KB value

            let start = Instant::now();
            for i in 0..ops_per_thread {
                // Generate random 16-byte key (UUID-like)
                let mut key = [0u8; 16];
                rng.fill(&mut key);
                // rng.fill(&mut value[..]); // Skip filling value to save CPU

                // Write
                db.put(&key, &value).unwrap();

                if i > 0 && i % 10_000 == 0 {
                    // Small sleep to prevent complete starvation of background threads in this synthetic test
                    // Real workloads have some think time.
                    // thread::sleep(Duration::from_micros(10));
                }
            }
            let elapsed = start.elapsed();
            (ops_per_thread, elapsed)
        });
        handles.push(handle);
    }

    // Monitor thread
    let db_monitor = db.clone();
    let monitor_handle = thread::spawn(move || {
        let start = Instant::now();
        loop {
            thread::sleep(Duration::from_secs(1));
            if start.elapsed().as_secs() > 60 {
                break;
            } // Safety timeout

            let stats = db_monitor.stats();
            let health = db_monitor.health();

            println!("\n[T+{:3}s] Throughput: {:.0} w/s | L0 Files: {} | Memtable: {:.1}% | Write Amp: {:.2}", 
                start.elapsed().as_secs(),
                stats.writes_per_sec,
                stats.sstables_per_level.get(0).unwrap_or(&0),
                stats.memtable_utilization_pct,
                stats.write_amplification
            );

            if !health.healthy {
                println!("  HEALTH WARNING: {:?}", health);
            }

            // Basic completion check (imprecise)
            if stats.total_puts >= total_ops as u64 {
                break;
            }
        }
    });

    // Wait for workers
    let mut total_duration = Duration::ZERO;
    for handle in handles {
        let (_, duration) = handle.join().unwrap();
        total_duration = total_duration.max(duration); // Max duration determines throughput
    }

    let total_elapsed = start_total.elapsed();
    let ops_per_sec = total_ops as f64 / total_elapsed.as_secs_f64();

    println!("\nTest Complete!");
    println!("Total time: {:.2}s", total_elapsed.as_secs_f64());
    println!("Throughput: {:.0} ops/sec", ops_per_sec);

    // Wait for monitor to finish printing final stats
    // monitor_handle.join().unwrap(); // Might hang if main finishes fast, let it die

    // Force flush and compact to see final state
    println!("Forcing final flush and compaction...");
    let _ = db.flush();

    let final_stats = db.stats();
    println!("Final Stats:");
    println!(
        "  L0 Files: {}",
        final_stats.sstables_per_level.get(0).unwrap_or(&0)
    );
    println!("  Total SSTables: {}", final_stats.total_sstables);
    println!(
        "  Write Amplification: {:.2}",
        final_stats.write_amplification
    );
}

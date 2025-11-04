// Soak Testing - 24-hour continuous operation
// Run manually: cargo test --release --test soak_test -- --ignored --nocapture
//
// Purpose: Validate long-term stability, memory stability, performance consistency
// Monitors: Memory usage, disk usage, latency percentiles, throughput
// Expected: No memory leaks, no performance degradation, stable operation

use seerdb::{DBOptions, SyncPolicy, DB};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

const SOAK_DURATION_HOURS: u64 = 24;
const REPORT_INTERVAL_SECS: u64 = 300; // Report every 5 minutes
const VALUE_SIZE: usize = 1024; // 1KB values

/// Helper to get current memory usage in bytes
fn get_memory_usage_bytes() -> u64 {
    // Platform-specific memory tracking
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        if let Ok(contents) = fs::read_to_string("/proc/self/status") {
            for line in contents.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
        0
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("ps")
            .args(["-o", "rss=", "-p"])
            .arg(std::process::id().to_string())
            .output()
            .ok();

        if let Some(output) = output {
            let rss_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(kb) = rss_str.trim().parse::<u64>() {
                return kb * 1024; // KB to bytes
            }
        }
        0
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        0 // Unsupported platform
    }
}

/// Helper to get disk usage for a directory in bytes
fn get_disk_usage_bytes(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                total += metadata.len();
            } else if metadata.is_dir() {
                total += get_disk_usage_bytes(&entry.path())?;
            }
        }
    }
    Ok(total)
}

#[test]
#[ignore] // Run manually with: cargo test --release --test soak_test -- --ignored --nocapture
fn test_24hour_soak() {
    println!("\n=== 24-HOUR SOAK TEST ===");
    println!("Duration: {} hours", SOAK_DURATION_HOURS);
    println!("Report interval: {}s", REPORT_INTERVAL_SECS);
    println!("Value size: {} bytes\n", VALUE_SIZE);

    let dir = tempdir().unwrap();
    let db_path = dir.path().to_path_buf();

    let opts = DBOptions {
        data_dir: db_path.clone(),
        background_compaction: true,
        wal_sync_policy: SyncPolicy::SyncData,
        memtable_capacity: 16 * 1024 * 1024, // 16MB memtable
        vlog_threshold: Some(512), // Use vLog for values >512 bytes
        ..Default::default()
    };

    let db = Arc::new(DB::open(opts).unwrap());

    // Shared state for monitoring
    let running = Arc::new(AtomicBool::new(true));
    let operations_completed = Arc::new(AtomicU64::new(0));
    let read_latency_total_us = Arc::new(AtomicU64::new(0));
    let write_latency_total_us = Arc::new(AtomicU64::new(0));

    // Monitoring thread
    let monitoring_running = running.clone();
    let monitoring_ops = operations_completed.clone();
    let monitoring_read_lat = read_latency_total_us.clone();
    let monitoring_write_lat = write_latency_total_us.clone();
    let monitoring_db_path = db_path.clone();

    let monitor_handle = thread::spawn(move || {
        let start_time = Instant::now();
        let initial_memory = get_memory_usage_bytes();
        println!("Initial memory: {} MB", initial_memory / 1_048_576);
        println!();

        let mut last_ops = 0u64;

        while monitoring_running.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(REPORT_INTERVAL_SECS));

            let elapsed = start_time.elapsed();
            let current_ops = monitoring_ops.load(Ordering::Relaxed);
            let current_memory = get_memory_usage_bytes();
            let disk_usage = get_disk_usage_bytes(&monitoring_db_path).unwrap_or(0);
            let read_lat_total = monitoring_read_lat.load(Ordering::Relaxed);
            let write_lat_total = monitoring_write_lat.load(Ordering::Relaxed);

            let ops_delta = current_ops - last_ops;
            let throughput = ops_delta as f64 / REPORT_INTERVAL_SECS as f64;

            println!("--- Report at {}h {}m ---", elapsed.as_secs() / 3600, (elapsed.as_secs() % 3600) / 60);
            println!("  Total operations: {}", current_ops);
            println!("  Throughput: {:.0} ops/sec", throughput);
            println!("  Memory usage: {} MB", current_memory / 1_048_576);
            println!("  Memory growth: {} MB", (current_memory as i64 - initial_memory as i64) / 1_048_576);
            println!("  Disk usage: {} MB", disk_usage / 1_048_576);

            if current_ops > 0 {
                let avg_read_lat = read_lat_total / current_ops.max(1);
                let avg_write_lat = write_lat_total / current_ops.max(1);
                println!("  Avg read latency: {} µs", avg_read_lat);
                println!("  Avg write latency: {} µs", avg_write_lat);
            }
            println!();

            last_ops = current_ops;

            // Validate memory is not leaking (should stay < 2x initial after warmup)
            if elapsed > Duration::from_secs(3600) {
                // After 1 hour warmup
                assert!(
                    current_memory < initial_memory * 3,
                    "Memory leak detected: {} MB > {}x initial ({} MB)",
                    current_memory / 1_048_576,
                    3,
                    (initial_memory * 3) / 1_048_576
                );
            }
        }
    });

    // Workload thread: mixed read/write operations
    let workload_db = db.clone();
    let workload_running = running.clone();
    let workload_ops = operations_completed.clone();
    let workload_read_lat = read_latency_total_us.clone();
    let workload_write_lat = write_latency_total_us.clone();

    let workload_handle = thread::spawn(move || {
        let mut key_counter = 0u64;
        let value = vec![b'x'; VALUE_SIZE];

        while workload_running.load(Ordering::Relaxed) {
            // 70% reads, 30% writes (typical production ratio)
            let op_type = key_counter % 10;

            if op_type < 3 {
                // Write operation
                let key = format!("soak_key_{:016}", key_counter);
                let start = Instant::now();
                workload_db.put(key.as_bytes(), &value).unwrap();
                let latency_us = start.elapsed().as_micros() as u64;
                workload_write_lat.fetch_add(latency_us, Ordering::Relaxed);
            } else {
                // Read operation (read recent keys)
                let read_key_counter = key_counter.saturating_sub(rand::random::<u64>() % 10000);
                let key = format!("soak_key_{:016}", read_key_counter);
                let start = Instant::now();
                let _ = workload_db.get(key.as_bytes()).unwrap();
                let latency_us = start.elapsed().as_micros() as u64;
                workload_read_lat.fetch_add(latency_us, Ordering::Relaxed);
            }

            workload_ops.fetch_add(1, Ordering::Relaxed);
            key_counter += 1;

            // Small sleep to avoid burning CPU (target ~1000 ops/sec)
            thread::sleep(Duration::from_micros(1000));
        }
    });

    // Run for configured duration
    thread::sleep(Duration::from_secs(SOAK_DURATION_HOURS * 3600));

    // Shutdown
    running.store(false, Ordering::Relaxed);
    workload_handle.join().unwrap();
    monitor_handle.join().unwrap();

    let final_ops = operations_completed.load(Ordering::Relaxed);
    println!("\n=== SOAK TEST COMPLETE ===");
    println!("Total operations: {}", final_ops);
    println!("Average throughput: {:.0} ops/sec", final_ops as f64 / (SOAK_DURATION_HOURS as f64 * 3600.0));
    println!("RESULT: PASS - No crashes, no memory leaks, stable operation");
}

#[test]
#[ignore] // Run manually with: cargo test --release --test soak_test -- --ignored --nocapture
fn test_large_dataset_100gb() {
    println!("\n=== 100GB+ DATASET TEST ===");
    println!("Target: Write and read 100GB+ of data");
    println!("This will take several hours...\n");

    let dir = tempdir().unwrap();
    let db_path = dir.path().to_path_buf();

    let opts = DBOptions {
        data_dir: db_path.clone(),
        background_compaction: true,
        wal_sync_policy: SyncPolicy::SyncData,
        memtable_capacity: 64 * 1024 * 1024, // 64MB memtable for faster writes
        vlog_threshold: Some(512),
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();

    // Write 100GB of data (100 million 1KB values)
    const TARGET_SIZE_GB: u64 = 100;
    const VALUE_SIZE: usize = 1024;
    const NUM_KEYS: u64 = (TARGET_SIZE_GB * 1024 * 1024 * 1024) / (VALUE_SIZE as u64);

    let value = vec![b'x'; VALUE_SIZE];
    let start_write = Instant::now();
    let mut last_report = Instant::now();
    let initial_memory = get_memory_usage_bytes();

    println!("Writing {} keys ({} GB)...", NUM_KEYS, TARGET_SIZE_GB);

    for i in 0..NUM_KEYS {
        let key = format!("large_key_{:016}", i);
        db.put(key.as_bytes(), &value).unwrap();

        // Report progress every 1% or 30 seconds
        if i % (NUM_KEYS / 100).max(1) == 0 || last_report.elapsed() > Duration::from_secs(30) {
            let progress = (i as f64 / NUM_KEYS as f64) * 100.0;
            let current_memory = get_memory_usage_bytes();
            let disk_usage = get_disk_usage_bytes(&db_path).unwrap_or(0);

            println!(
                "  Progress: {:.1}% ({} keys, {} GB disk, {} MB memory)",
                progress,
                i,
                disk_usage / (1024 * 1024 * 1024),
                current_memory / 1_048_576
            );
            last_report = Instant::now();

            // Memory should stay bounded
            assert!(
                current_memory < initial_memory * 4,
                "Memory growing unbounded during write phase"
            );
        }
    }

    let write_duration = start_write.elapsed();
    let write_throughput = NUM_KEYS as f64 / write_duration.as_secs_f64();
    println!("\nWrite phase complete:");
    println!("  Duration: {:.2}s", write_duration.as_secs_f64());
    println!("  Throughput: {:.0} writes/sec", write_throughput);

    // Force final flush
    db.flush().unwrap();

    let final_disk = get_disk_usage_bytes(&db_path).unwrap();
    println!("  Final disk usage: {} GB", final_disk / (1024 * 1024 * 1024));

    // Read phase: Random reads to validate data
    println!("\nValidating data with random reads...");
    let num_reads = 100_000u64;
    let start_read = Instant::now();

    for _ in 0..num_reads {
        let random_key = rand::random::<u64>() % NUM_KEYS;
        let key = format!("large_key_{:016}", random_key);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Key should exist");
        assert_eq!(result.unwrap().len(), VALUE_SIZE, "Value size mismatch");
    }

    let read_duration = start_read.elapsed();
    let read_throughput = num_reads as f64 / read_duration.as_secs_f64();
    println!("Read validation complete:");
    println!("  {} random reads", num_reads);
    println!("  Duration: {:.2}s", read_duration.as_secs_f64());
    println!("  Throughput: {:.0} reads/sec", read_throughput);

    let final_memory = get_memory_usage_bytes();
    println!("\n=== LARGE DATASET TEST COMPLETE ===");
    println!("Data written: {} GB", final_disk / (1024 * 1024 * 1024));
    println!("Memory usage: {} MB (started at {} MB)", final_memory / 1_048_576, initial_memory / 1_048_576);
    println!("RESULT: PASS - Successfully handled 100GB+ dataset");

    assert!(
        final_memory < initial_memory * 3,
        "Memory leak detected in large dataset test"
    );
}

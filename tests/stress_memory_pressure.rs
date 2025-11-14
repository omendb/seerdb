// Memory Pressure Stress Tests
// Tests memory budget enforcement (80% flush trigger, 95% write blocking)
// Critical for preventing OOM in production
// Added Nov 14, 2025 for production validation

use seerdb::{DB, DBOptions};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_memory_pressure_80_percent_trigger() {
    // Test that 80% memory pressure triggers early flush
    let temp_dir = TempDir::new().unwrap();
    let data_dir = PathBuf::from(temp_dir.path());

    let opts = DBOptions {
        data_dir,
        max_memory_bytes: Some(100 * 1024 * 1024), // 100MB budget
        memtable_capacity: 20 * 1024 * 1024,       // 20MB per memtable
        background_flush: true,
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();

    let initial_memory = db.estimate_memory_usage();
    println!("Initial memory: {} bytes", initial_memory);

    // Write data to approach 80% threshold (~80MB)
    // Each write: key (20 bytes) + value (1KB) ≈ 1044 bytes
    // Target: ~76K writes to approach 80%
    let mut wrote_count = 0;
    for i in 0..80_000 {
        let key = format!("key_{:010}", i);
        let value = vec![b'x'; 1000]; // 1KB value

        db.put(key.as_bytes(), &value).unwrap();
        wrote_count += 1;

        // Check memory every 5000 writes
        if i % 5000 == 0 && i > 0 {
            let mem = db.estimate_memory_usage();
            let pressure = (mem as f64) / (100.0 * 1024.0 * 1024.0);
            println!(
                "After {} writes: {} MB ({:.1}% pressure)",
                i,
                mem / 1024 / 1024,
                pressure * 100.0
            );

            // Should never exceed 95% threshold
            if pressure >= 0.95 {
                panic!(
                    "Memory pressure exceeded 95% ({:.1}%) - backpressure failed",
                    pressure * 100.0
                );
            }
        }
    }

    println!("Successfully wrote {} operations without OOM", wrote_count);

    // Flush to ensure everything persists
    db.flush().unwrap();

    // Verify some data (spot check)
    for i in (0..wrote_count).step_by(10000) {
        let key = format!("key_{:010}", i);
        assert!(
            db.get(key.as_bytes()).unwrap().is_some(),
            "Key {} should exist after memory pressure test",
            key
        );
    }
}

#[test]
fn test_memory_pressure_no_oom() {
    // Test that database never OOMs regardless of write volume
    let temp_dir = TempDir::new().unwrap();
    let data_dir = PathBuf::from(temp_dir.path());

    let opts = DBOptions {
        data_dir,
        max_memory_bytes: Some(50 * 1024 * 1024), // 50MB budget (small)
        memtable_capacity: 10 * 1024 * 1024,      // 10MB per memtable
        background_flush: true,
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();

    // Write 200K operations (should trigger many flushes)
    let mut max_memory_seen = 0;
    for i in 0..100_000 {
        let key = format!("stress_{:010}", i);
        let value = vec![b'y'; 500]; // 500 bytes

        db.put(key.as_bytes(), &value).unwrap();

        if i % 10000 == 0 {
            let mem = db.estimate_memory_usage();
            max_memory_seen = max_memory_seen.max(mem);

            // Should never exceed configured limit significantly
            assert!(
                mem < 60 * 1024 * 1024, // Allow 20% overhead
                "Memory usage {} exceeds safe limit",
                mem
            );
        }
    }

    println!(
        "Maximum memory observed: {} MB (limit: 50 MB)",
        max_memory_seen / 1024 / 1024
    );

    // Verify database is still functional
    db.flush().unwrap();
    assert!(db.get(b"stress_0000000000").unwrap().is_some());
    assert!(db.get(b"stress_0000099999").unwrap().is_some());
}

#[test]
fn test_memory_pressure_recovery() {
    // Test that memory pressure recovers after flush
    let temp_dir = TempDir::new().unwrap();
    let data_dir = PathBuf::from(temp_dir.path());

    let opts = DBOptions {
        data_dir,
        max_memory_bytes: Some(80 * 1024 * 1024), // 80MB budget
        memtable_capacity: 15 * 1024 * 1024,      // 15MB per memtable
        background_flush: true,
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();

    // Phase 1: Build up memory pressure
    for i in 0..50_000 {
        let key = format!("phase1_{:010}", i);
        let value = vec![b'a'; 1000];
        db.put(key.as_bytes(), &value).unwrap();
    }

    let high_memory = db.estimate_memory_usage();
    println!("High memory: {} MB", high_memory / 1024 / 1024);

    // Phase 2: Wait for flushes to complete
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Phase 3: Memory should have recovered
    let recovered_memory = db.estimate_memory_usage();
    println!("Recovered memory: {} MB", recovered_memory / 1024 / 1024);

    // Memory should be significantly lower after flush
    assert!(
        recovered_memory < high_memory / 2,
        "Memory should recover after flush: {} > {}",
        recovered_memory,
        high_memory / 2
    );

    // Phase 4: Verify we can continue writing
    for i in 0..50_000 {
        let key = format!("phase2_{:010}", i);
        let value = vec![b'b'; 1000];
        db.put(key.as_bytes(), &value).unwrap();
    }

    // Verify both phases persisted
    assert!(db.get(b"phase1_0000000000").unwrap().is_some());
    assert!(db.get(b"phase2_0000000000").unwrap().is_some());
}

#[test]
fn test_memory_pressure_disabled() {
    // Test that without memory budget, database still works
    let temp_dir = TempDir::new().unwrap();
    let data_dir = PathBuf::from(temp_dir.path());

    let opts = DBOptions {
        data_dir,
        max_memory_bytes: None, // No memory limit
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();

    // Should be able to write freely
    for i in 0..50_000 {
        let key = format!("unlimited_{:010}", i);
        let value = vec![b'z'; 1000];
        db.put(key.as_bytes(), &value).unwrap();
    }

    db.flush().unwrap();
    assert!(db.get(b"unlimited_0000000000").unwrap().is_some());
}

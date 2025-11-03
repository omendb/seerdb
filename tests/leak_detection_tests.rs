use seerdb::{DBOptions, DB};
use std::path::PathBuf;
use std::time::Duration;
use sysinfo::{Pid, ProcessExt, System, SystemExt};
use tempfile::TempDir;

/// Get current process memory usage in bytes
fn get_memory_usage() -> u64 {
    let mut sys = System::new_all();
    sys.refresh_all();

    let pid = Pid::from(std::process::id() as usize);
    sys.process(pid).map(|p| p.memory()).unwrap_or(0)
}

/// Get current process file descriptor count (Unix-like systems)
#[cfg(unix)]
fn get_fd_count() -> usize {
    let pid = std::process::id();

    // On macOS, /proc doesn't exist, so we'll use lsof
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("lsof").arg("-p").arg(pid.to_string()).output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Count lines (minus header)
            stdout.lines().count().saturating_sub(1)
        } else {
            0
        }
    }

    // On Linux, count /proc/self/fd entries
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        let fd_dir = format!("/proc/{}/fd", pid);
        fs::read_dir(&fd_dir)
            .map(|entries| entries.count())
            .unwrap_or(0)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

/// Get current thread count
#[cfg(unix)]
fn get_thread_count() -> usize {
    let pid = std::process::id();

    #[cfg(target_os = "macos")]
    {
        // On macOS, use ps to count threads
        use std::process::Command;
        let output = Command::new("ps")
            .arg("-M")
            .arg("-p")
            .arg(pid.to_string())
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Count lines (minus header)
            stdout.lines().count().saturating_sub(1)
        } else {
            1 // At least one thread (main)
        }
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, count tasks in /proc/pid/task
        use std::fs;
        let task_dir = format!("/proc/{}/task", pid);
        fs::read_dir(&task_dir)
            .map(|entries| entries.count())
            .unwrap_or(1)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        1
    }
}

#[cfg(not(unix))]
fn get_thread_count() -> usize {
    1 // Fallback for non-Unix systems
}

#[test]
fn test_no_memory_leak_sequential_writes() {
    let temp_dir = TempDir::new().unwrap();
    let opts = DBOptions {
        data_dir: PathBuf::from(temp_dir.path()),
        memtable_capacity: 4 * 1024 * 1024, // 4MB memtable
        ..Default::default()
    };
    let db = DB::open(opts).unwrap();

    // Baseline memory
    let baseline_memory = get_memory_usage();
    println!("Baseline memory: {} MB", baseline_memory / 1024 / 1024);

    let mut memory_samples = Vec::new();
    memory_samples.push(baseline_memory);

    // Write 100k operations with periodic flushes
    for i in 0..100_000 {
        let key = format!("key{:07}", i);
        let value = vec![b'v'; 100]; // 100 bytes per value
        db.put(key.as_bytes(), &value).unwrap();

        // Sample memory every 10k operations
        if i % 10_000 == 0 {
            let current_memory = get_memory_usage();
            memory_samples.push(current_memory);
            println!("After {} ops: {} MB", i, current_memory / 1024 / 1024);
        }
    }

    // Final memory check
    let final_memory = get_memory_usage();
    memory_samples.push(final_memory);
    println!("Final memory: {} MB", final_memory / 1024 / 1024);

    // Calculate growth
    let max_memory = *memory_samples.iter().max().unwrap();
    let min_memory = *memory_samples.iter().min().unwrap();
    let growth_ratio = max_memory as f64 / min_memory as f64;

    println!(
        "Memory growth: {:.2}x ({} MB -> {} MB)",
        growth_ratio,
        min_memory / 1024 / 1024,
        max_memory / 1024 / 1024
    );

    // Memory should not grow more than 3x (allowing for caching and memtable)
    assert!(
        growth_ratio < 3.0,
        "Possible memory leak: {:.2}x growth ({} MB -> {} MB)",
        growth_ratio,
        min_memory / 1024 / 1024,
        max_memory / 1024 / 1024
    );
}

#[test]
fn test_no_memory_leak_repeated_flushes() {
    let temp_dir = TempDir::new().unwrap();
    let opts = DBOptions {
        data_dir: PathBuf::from(temp_dir.path()),
        memtable_capacity: 1024 * 1024, // 1MB memtable (small, triggers frequent flushes)
        ..Default::default()
    };
    let db = DB::open(opts).unwrap();

    let baseline_memory = get_memory_usage();
    println!("Baseline memory: {} MB", baseline_memory / 1024 / 1024);

    let mut memory_samples = Vec::new();
    memory_samples.push(baseline_memory);

    // Trigger 50 flushes
    for flush_num in 0..50 {
        // Write enough to trigger flush
        for i in 0..1000 {
            let key = format!("f{}_k{:04}", flush_num, i);
            let value = vec![b'v'; 200]; // 200 bytes
            db.put(key.as_bytes(), &value).unwrap();
        }

        // Explicit flush
        db.flush().unwrap();

        // Sample memory after each flush
        let current_memory = get_memory_usage();
        memory_samples.push(current_memory);

        if flush_num % 10 == 0 {
            println!(
                "After flush {}: {} MB",
                flush_num,
                current_memory / 1024 / 1024
            );
        }
    }

    let max_memory = *memory_samples.iter().max().unwrap();
    let min_memory = *memory_samples.iter().min().unwrap();
    let growth_ratio = max_memory as f64 / min_memory as f64;

    println!(
        "Memory growth after 50 flushes: {:.2}x ({} MB -> {} MB)",
        growth_ratio,
        min_memory / 1024 / 1024,
        max_memory / 1024 / 1024
    );

    // Memory should be relatively stable after flushes
    assert!(
        growth_ratio < 2.5,
        "Memory leak in flush: {:.2}x growth",
        growth_ratio
    );
}

#[test]
fn test_no_memory_leak_put_delete_cycles() {
    let temp_dir = TempDir::new().unwrap();
    let opts = DBOptions {
        data_dir: PathBuf::from(temp_dir.path()),
        ..Default::default()
    };
    let db = DB::open(opts).unwrap();

    let baseline_memory = get_memory_usage();
    let mut memory_samples = vec![baseline_memory];

    // Rapid put/delete cycles
    for cycle in 0..100 {
        // Put 1000 keys
        for i in 0..1000 {
            let key = format!("key{:04}", i);
            db.put(key.as_bytes(), b"value").unwrap();
        }

        // Delete 1000 keys
        for i in 0..1000 {
            let key = format!("key{:04}", i);
            db.delete(key.as_bytes()).unwrap();
        }

        if cycle % 20 == 0 {
            let current_memory = get_memory_usage();
            memory_samples.push(current_memory);
        }
    }

    let max_memory = *memory_samples.iter().max().unwrap();
    let min_memory = *memory_samples.iter().min().unwrap();
    let growth_ratio = max_memory as f64 / min_memory as f64;

    println!(
        "Memory growth after 100 put/delete cycles: {:.2}x",
        growth_ratio
    );

    assert!(
        growth_ratio < 2.0,
        "Memory leak in put/delete cycles: {:.2}x growth",
        growth_ratio
    );
}

#[test]
#[cfg(unix)]
fn test_no_fd_leak_db_open_close() {
    // Get baseline FD count
    let baseline_fds = get_fd_count();
    println!("Baseline FD count: {}", baseline_fds);

    // Open and close DB 20 times
    for i in 0..20 {
        let temp_dir = TempDir::new().unwrap();
        let opts = DBOptions {
            data_dir: PathBuf::from(temp_dir.path()),
            ..Default::default()
        };

        {
            let db = DB::open(opts).unwrap();

            // Write some data
            for j in 0..100 {
                db.put(format!("key{}", j).as_bytes(), b"value").unwrap();
            }

            db.flush().unwrap();

            // DB dropped here
        }

        // TempDir cleaned up here

        if i % 5 == 0 {
            let current_fds = get_fd_count();
            println!("After {} cycles: {} FDs", i + 1, current_fds);
        }
    }

    // Check final FD count
    std::thread::sleep(Duration::from_millis(100)); // Give OS time to close files
    let final_fds = get_fd_count();
    println!("Final FD count: {}", final_fds);

    // FD count should return to baseline (allow small variance)
    let fd_growth = final_fds as i32 - baseline_fds as i32;
    assert!(
        fd_growth.abs() < 10,
        "Possible FD leak: {} FDs leaked",
        fd_growth
    );
}

#[test]
#[cfg(unix)]
fn test_no_fd_leak_multiple_flushes() {
    let temp_dir = TempDir::new().unwrap();
    let opts = DBOptions {
        data_dir: PathBuf::from(temp_dir.path()),
        ..Default::default()
    };

    let baseline_fds = get_fd_count();
    println!("Baseline FD count: {}", baseline_fds);

    let db = DB::open(opts).unwrap();

    // After opening, expect some FDs (WAL, data dir)
    let db_open_fds = get_fd_count();
    println!("FD count after DB open: {}", db_open_fds);

    // Multiple flushes (creates SSTables)
    for i in 0..30 {
        for j in 0..1000 {
            db.put(format!("f{}_k{}", i, j).as_bytes(), b"value")
                .unwrap();
        }
        db.flush().unwrap();

        if i % 10 == 0 {
            let current_fds = get_fd_count();
            println!("After flush {}: {} FDs", i, current_fds);
        }
    }

    let after_flushes_fds = get_fd_count();
    println!("FD count after 30 flushes: {}", after_flushes_fds);

    drop(db);
    std::thread::sleep(Duration::from_millis(100));

    let final_fds = get_fd_count();
    println!("FD count after DB drop: {}", final_fds);

    // After drop, should return close to baseline
    let fd_growth = final_fds as i32 - baseline_fds as i32;
    assert!(
        fd_growth.abs() < 10,
        "FD leak after drop: {} FDs leaked",
        fd_growth
    );
}

#[test]
fn test_no_thread_leak_db_lifecycle() {
    let baseline_threads = get_thread_count();
    println!("Baseline thread count: {}", baseline_threads);

    // Open DB with background compaction disabled (simpler case first)
    {
        let temp_dir = TempDir::new().unwrap();
        let opts = DBOptions {
            data_dir: PathBuf::from(temp_dir.path()),
            background_compaction: false,
            ..Default::default()
        };

        let db = DB::open(opts).unwrap();

        for i in 0..1000 {
            db.put(format!("key{}", i).as_bytes(), b"value").unwrap();
        }

        db.flush().unwrap();

        // DB dropped here
    }

    std::thread::sleep(Duration::from_millis(200));

    let final_threads = get_thread_count();
    println!("Final thread count: {}", final_threads);

    // Thread count should return to baseline
    assert_eq!(
        final_threads, baseline_threads,
        "Thread leak detected: {} threads -> {} threads",
        baseline_threads, final_threads
    );
}

#[test]
#[ignore] // Only run manually - tests background thread cleanup
fn test_no_thread_leak_background_compaction() {
    let baseline_threads = get_thread_count();
    println!("Baseline thread count: {}", baseline_threads);

    {
        let temp_dir = TempDir::new().unwrap();
        let opts = DBOptions {
            data_dir: PathBuf::from(temp_dir.path()),
            background_compaction: true, // Enable background thread
            ..Default::default()
        };

        let db = DB::open(opts).unwrap();

        let during_threads = get_thread_count();
        println!("Thread count with DB open: {}", during_threads);

        // Should have at least one more thread for background compaction
        assert!(
            during_threads > baseline_threads,
            "Background compaction thread not started"
        );

        // Write data to trigger compaction
        for i in 0..10_000 {
            db.put(format!("key{:05}", i).as_bytes(), b"value").unwrap();
        }

        db.flush().unwrap();

        // DB dropped here - background thread should be joined
    }

    // Give background thread time to shut down
    std::thread::sleep(Duration::from_millis(500));

    let final_threads = get_thread_count();
    println!("Final thread count after drop: {}", final_threads);

    // Thread count should return to baseline
    assert_eq!(
        final_threads, baseline_threads,
        "Background thread not cleaned up: {} threads -> {} threads",
        baseline_threads, final_threads
    );
}

#[test]
fn test_memory_stable_after_reopen() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = PathBuf::from(temp_dir.path());

    // First session: write data
    {
        let opts = DBOptions {
            data_dir: db_path.clone(),
            ..Default::default()
        };
        let db = DB::open(opts).unwrap();

        for i in 0..10_000 {
            db.put(format!("key{:05}", i).as_bytes(), b"value").unwrap();
        }

        db.flush().unwrap();
    }

    let baseline_memory = get_memory_usage();

    // Second session: reopen and read
    {
        let opts = DBOptions {
            data_dir: db_path.clone(),
            ..Default::default()
        };
        let db = DB::open(opts).unwrap();

        // Read all data
        for i in 0..10_000 {
            let _ = db.get(format!("key{:05}", i).as_bytes()).unwrap();
        }
    }

    let after_reopen_memory = get_memory_usage();
    let growth_ratio = after_reopen_memory as f64 / baseline_memory as f64;

    println!(
        "Memory after reopen: {:.2}x baseline ({} MB -> {} MB)",
        growth_ratio,
        baseline_memory / 1024 / 1024,
        after_reopen_memory / 1024 / 1024
    );

    // Memory should not grow significantly after reopen
    assert!(
        growth_ratio < 1.5,
        "Memory leak on reopen: {:.2}x growth",
        growth_ratio
    );
}

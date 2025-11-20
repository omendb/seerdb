
use seerdb::{DBOptions, SyncPolicy, DB};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

const THREADS: usize = 50;
const WRITES_PER_THREAD: usize = 1000;

fn main() {
    println!("=== WAL Bottleneck Analysis ===");
    println!("Threads: {}", THREADS);
    println!("Writes per thread: {}", WRITES_PER_THREAD);
    
    // 1. SyncPolicy::None (Upper bound)
    let none_throughput = run_test("SyncNone", SyncPolicy::None, 0, THREADS);
    
    // 2. SyncPolicy::SyncData with Delay=0 (Baseline - no group commit)
    let base_throughput = run_test("SyncData (0us)", SyncPolicy::SyncData, 0, THREADS);
    
    // 3. SyncPolicy::SyncData with Delay=200us (Current Group Commit)
    let gc_throughput_1 = run_test("SyncData (1th, 200us)", SyncPolicy::SyncData, 200, 1);
    let gc_throughput_50 = run_test("SyncData (50th, 200us)", SyncPolicy::SyncData, 200, 50);
    
    println!("\n=== Analysis ===");
    println!("SyncNone (Max Speed): {:.0} ops/sec", none_throughput);
    println!("SyncData (Baseline):  {:.0} ops/sec", base_throughput);
    println!("SyncData (GroupGC 1): {:.0} ops/sec", gc_throughput_1);
    println!("SyncData (GroupGC 50):{:.0} ops/sec", gc_throughput_50);
    
    println!("\nScaling:");
    println!("50 threads vs 1 thread: {:.2}x (Ideal: 50x)", gc_throughput_50 / gc_throughput_1);
}

fn run_test(name: &str, policy: SyncPolicy, delay_us: u64, threads: usize) -> f64 {
    let dir = tempdir().unwrap();
    
    let opts = DBOptions {
        data_dir: dir.path().to_path_buf(),
        wal_sync_policy: policy,
        group_commit_delay_us: delay_us,
        group_commit_max_batch_size: 1000,
        memtable_capacity: 64 * 1024 * 1024,
        ..Default::default()
    };
    
    let db = Arc::new(DB::open(opts).unwrap());
    
    let start = Instant::now();
    
    let handles: Vec<_> = (0..threads).map(|id| {
        let db = db.clone();
        thread::spawn(move || {
            for i in 0..WRITES_PER_THREAD {
                let key = format!("key:{}:{}", id, i);
                let value = vec![0u8; 100]; // 100 bytes
                db.put(key, value).unwrap();
            }
        })
    }).collect();
    
    for h in handles {
        h.join().unwrap();
    }
    
    let duration = start.elapsed();
    let ops = (threads * WRITES_PER_THREAD) as f64;
    let throughput = ops / duration.as_secs_f64();
    
    println!("{:<20}: {:.0} ops/sec ({:.2}s)", name, throughput, duration.as_secs_f64());
    
    throughput
}

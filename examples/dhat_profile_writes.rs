// Allocation profiling for write-heavy workload
//
// Run with: cargo run --release --example dhat_profile_writes
// View results: https://nnethercote.github.io/dh_view/dh_view.html
// Upload the generated dhat-heap.json file to the viewer

use seerdb::{DBOptions, DB};
use std::time::Instant;
use tempfile::tempdir;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    let _profiler = dhat::Profiler::new_heap();

    println!("=== dhat Allocation Profiling: Write-Heavy Workload ===\n");

    let dir = tempdir().unwrap();
    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 4 * 1024 * 1024, // 4MB memtable
        block_cache_capacity: 16_384,
        ..Default::default()
    };

    let db = DB::open(options).unwrap();

    // Write-heavy workload: 100K sequential writes
    println!("Phase 1: Sequential writes (100K entries)");
    let start = Instant::now();

    for i in 0..100_000 {
        let key = format!("key:{:08}", i);
        let value = format!("value_data_{}", i);
        db.put(key.as_bytes(), value.as_bytes()).unwrap();

        if (i + 1) % 10_000 == 0 {
            print!("\rProgress: {}/100000", i + 1);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }

    let write_elapsed = start.elapsed();
    println!("\nPhase 1 complete: {:?}", write_elapsed);
    println!("Throughput: {:.0} writes/sec\n", 100_000.0 / write_elapsed.as_secs_f64());

    // Force flush to trigger SSTable creation
    println!("Phase 2: Flushing to SSTables");
    let flush_start = Instant::now();
    db.flush().unwrap();
    println!("Flush complete: {:?}\n", flush_start.elapsed());

    // Batch writes
    println!("Phase 3: Batch writes (10K entries in batches of 100)");
    let batch_start = Instant::now();

    for batch_idx in 0..100 {
        let mut batch = db.batch();

        for i in 0..100 {
            let key = format!("batch:{}:{:05}", batch_idx, i);
            let value = format!("batch_value_{}", i);
            batch.put(key.as_bytes(), value.as_bytes());
        }

        batch.commit().unwrap();
    }

    let batch_elapsed = batch_start.elapsed();
    println!("Phase 3 complete: {:?}", batch_elapsed);
    println!("Throughput: {:.0} writes/sec\n", 10_000.0 / batch_elapsed.as_secs_f64());

    // Random writes (may trigger compaction)
    println!("Phase 4: Random writes (50K entries)");
    let random_start = Instant::now();

    for i in 0..50_000 {
        let random_key = rand::random::<u64>();
        let key = format!("random:{:016x}", random_key);
        let value = format!("random_value_{}", i);
        db.put(key.as_bytes(), value.as_bytes()).unwrap();

        if (i + 1) % 10_000 == 0 {
            print!("\rProgress: {}/50000", i + 1);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }

    let random_elapsed = random_start.elapsed();
    println!("\nPhase 4 complete: {:?}", random_elapsed);
    println!("Throughput: {:.0} writes/sec\n", 50_000.0 / random_elapsed.as_secs_f64());

    // Get final stats
    let stats = db.stats();
    println!("=== Final Statistics ===");
    println!("Total writes: {}", stats.total_puts);
    println!("Total flushes: {}", stats.total_flushes);
    println!("Total compactions: {}", stats.total_compactions);
    println!("Total SSTables: {}", stats.total_sstables);
    println!("Memtable size: {:.2} MB", stats.memtable_size_bytes as f64 / 1024.0 / 1024.0);
    println!("Total disk usage: {:.2} MB", stats.total_disk_bytes as f64 / 1024.0 / 1024.0);
    println!("Write amplification: {:.2}x", stats.write_amplification);

    println!("\n=== Allocation Profile Complete ===");
    println!("dhat-heap.json written to current directory");
    println!("View at: https://nnethercote.github.io/dh_view/dh_view.html");
}

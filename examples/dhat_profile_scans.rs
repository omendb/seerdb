// Allocation profiling for scan-heavy workload
//
// Run with: cargo run --release --example dhat_profile_scans
// View results: https://nnethercote.github.io/dh_view/dh_view.html
// Upload the generated dhat-heap.json file to the viewer

use seerdb::{DBOptions, DB};
use std::time::Instant;
use tempfile::tempdir;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    let _profiler = dhat::Profiler::new_heap();

    println!("=== dhat Allocation Profiling: Scan-Heavy Workload ===\n");

    let dir = tempdir().unwrap();
    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 4 * 1024 * 1024, // 4MB memtable
        block_cache_capacity: 16_384,
        ..Default::default()
    };

    let db = DB::open(options).unwrap();

    // Setup: Write data to scan over
    println!("Setup: Writing 100K entries across multiple prefixes");
    let setup_start = Instant::now();

    // Create data with multiple prefixes for prefix scans
    for prefix_idx in 0..100 {
        for i in 0..1000 {
            let key = format!("prefix:{:03}:key:{:05}", prefix_idx, i);
            let value = format!("value_data_{}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        if (prefix_idx + 1) % 10 == 0 {
            print!("\rProgress: {}/100 prefixes", prefix_idx + 1);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }

    println!("\nSetup complete: {:?}", setup_start.elapsed());
    db.flush().unwrap();
    println!("Data flushed to SSTables\n");

    // Phase 1: Full table scans
    println!("Phase 1: Full table scans (5 iterations)");
    let full_scan_start = Instant::now();
    let mut total_keys = 0usize;

    for iteration in 0..5 {
        let iter_start = Instant::now();
        let mut count = 0;

        for result in db.iter().unwrap() {
            let (_key, _value) = result.unwrap();
            count += 1;
        }

        total_keys += count;
        println!("  Iteration {}: {} keys in {:?}", iteration + 1, count, iter_start.elapsed());
    }

    println!("Phase 1 complete: {:?}", full_scan_start.elapsed());
    println!("Avg throughput: {:.0} keys/sec\n", total_keys as f64 / full_scan_start.elapsed().as_secs_f64());

    // Phase 2: Range scans (various sizes)
    println!("Phase 2: Range scans (1000 scans of varying sizes)");
    let range_scan_start = Instant::now();
    let mut range_keys = 0usize;

    for i in 0..1000 {
        let start_key = format!("prefix:{:03}:", i % 100);
        let end_key = format!("prefix:{:03}:{}", i % 100, char::from_u32(0xFF).unwrap());

        for result in db.range(start_key.as_bytes(), Some(end_key.as_bytes())).unwrap() {
            let (_key, _value) = result.unwrap();
            range_keys += 1;
        }

        if (i + 1) % 100 == 0 {
            print!("\rProgress: {}/1000 scans", i + 1);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }

    println!("\nPhase 2 complete: {:?}", range_scan_start.elapsed());
    println!("Keys scanned: {}, throughput: {:.0} keys/sec\n",
             range_keys, range_keys as f64 / range_scan_start.elapsed().as_secs_f64());

    // Phase 3: Prefix scans (omendb-like pattern)
    println!("Phase 3: Prefix scans (5000 small prefix scans)");
    let prefix_scan_start = Instant::now();
    let mut prefix_keys = 0usize;

    for i in 0..5000 {
        let prefix = format!("prefix:{:03}:key:{:02}", i % 100, i % 100);

        for result in db.prefix(prefix.as_bytes()).unwrap() {
            let (_key, _value) = result.unwrap();
            prefix_keys += 1;
        }

        if (i + 1) % 500 == 0 {
            print!("\rProgress: {}/5000 scans", i + 1);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }

    println!("\nPhase 3 complete: {:?}", prefix_scan_start.elapsed());
    println!("Keys scanned: {}, throughput: {:.0} keys/sec\n",
             prefix_keys, prefix_keys as f64 / prefix_scan_start.elapsed().as_secs_f64());

    // Phase 4: Keys-only iteration (measure allocation difference)
    println!("Phase 4: Keys-only iteration (1000 scans)");
    let keys_only_start = Instant::now();
    let mut keys_only_count = 0usize;

    for i in 0..1000 {
        let prefix = format!("prefix:{:03}:", i % 100);

        for result in db.prefix_keys_only(prefix.as_bytes()).unwrap() {
            let (_key, _) = result.unwrap();
            keys_only_count += 1;
        }

        if (i + 1) % 100 == 0 {
            print!("\rProgress: {}/1000 scans", i + 1);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }

    println!("\nPhase 4 complete: {:?}", keys_only_start.elapsed());
    println!("Keys scanned: {}, throughput: {:.0} keys/sec\n",
             keys_only_count, keys_only_count as f64 / keys_only_start.elapsed().as_secs_f64());

    // Get final stats
    let stats = db.stats();
    println!("=== Final Statistics ===");
    println!("Cache hit rate: {:.2}%", stats.cache_hit_rate * 100.0);
    println!("Cache hits: {}", stats.cache_hits);
    println!("Cache misses: {}", stats.cache_misses);
    println!("Block cache size: {} / {}", stats.block_cache_size, stats.block_cache_capacity);

    println!("\n=== Allocation Profile Complete ===");
    println!("dhat-heap.json written to current directory");
    println!("View at: https://nnethercote.github.io/dh_view/dh_view.html");
}

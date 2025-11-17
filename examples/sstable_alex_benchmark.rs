// Benchmark SSTable lookups with ALEX learned index
//
// This benchmark measures the impact of ALEX on SSTable top-level index lookups.
// Expected: 1.5x faster lookups with ALEX vs binary search

use seerdb::{DBOptions, DB};
use std::time::Instant;
use tempfile::TempDir;

fn main() {
    println!("╔═══════════════════════════════════════════════════════╗");
    println!("║   SSTable ALEX Benchmark                              ║");
    println!("║   Measuring top-level index lookup performance       ║");
    println!("╚═══════════════════════════════════════════════════════╝");
    println!();

    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Test parameters
    let num_entries = 10_000;
    let num_lookups = 10_000;

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Setup");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Entries:       {}", num_entries);
    println!("Lookups:       {}", num_lookups);
    println!("Value size:    128 bytes");
    println!();

    // Create database and insert data
    {
        let opts = DBOptions {
            data_dir: data_dir.to_path_buf(),
            memtable_capacity: 64 * 1024 * 1024, // 64MB memtable
            ..Default::default()
        };
        let db = DB::open(opts).unwrap();

        print!("Writing {} entries...", num_entries);
        for i in 0..num_entries {
            let key = format!("key_{:08}", i);
            let value = vec![b'x'; 128];
            db.put(key.as_bytes(), &value).unwrap();

            if (i + 1) % (num_entries / 10) == 0 {
                print!(" {}%", (i + 1) * 100 / num_entries);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            }
        }
        println!(" Done");

        // Force flush to create SSTables
        print!("Flushing to SSTables...");
        db.flush().unwrap();
        println!(" Done");
    }

    // Reopen database (this builds ALEX index)
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Benchmark: Random Lookups");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    let opts = DBOptions {
        data_dir: data_dir.to_path_buf(),
        ..Default::default()
    };
    let db = DB::open(opts).unwrap();

    // Benchmark random lookups
    let mut rng_state = 12345u64;
    let start = Instant::now();
    let mut found = 0;

    for _ in 0..num_lookups {
        // Simple LCG for deterministic random numbers
        rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        let idx = (rng_state % num_entries as u64) as usize;

        let key = format!("key_{:08}", idx);
        if db.get(key.as_bytes()).unwrap().is_some() {
            found += 1;
        }
    }

    let duration = start.elapsed();
    let ns_per_lookup = duration.as_nanos() / num_lookups as u128;
    let lookups_per_sec = (num_lookups as f64 / duration.as_secs_f64()) as u64;

    println!("Results:");
    println!("  Total time:       {:?}", duration);
    println!("  Lookups/sec:      {}", lookups_per_sec);
    println!("  Latency:          {} ns/lookup", ns_per_lookup);
    println!("  Found:            {}/{}", found, num_lookups);
    println!();

    // Sequential lookups (best case for ALEX)
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Benchmark: Sequential Lookups");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    let start = Instant::now();
    let mut found = 0;

    for i in 0..num_lookups {
        let key = format!("key_{:08}", i);
        if db.get(key.as_bytes()).unwrap().is_some() {
            found += 1;
        }
    }

    let duration = start.elapsed();
    let ns_per_lookup = duration.as_nanos() / num_lookups as u128;
    let lookups_per_sec = (num_lookups as f64 / duration.as_secs_f64()) as u64;

    println!("Results:");
    println!("  Total time:       {:?}", duration);
    println!("  Lookups/sec:      {}", lookups_per_sec);
    println!("  Latency:          {} ns/lookup", ns_per_lookup);
    println!("  Found:            {}/{}", found, num_lookups);
    println!();

    println!("╔═══════════════════════════════════════════════════════╗");
    println!("║   Analysis                                            ║");
    println!("╚═══════════════════════════════════════════════════════╝");
    println!();
    println!("ALEX learned index is integrated into SSTable top-level index.");
    println!("Previous benchmarks showed 1.08-1.54x speedup for standalone ALEX.");
    println!();
    println!("Note: To measure pure ALEX impact, would need baseline without ALEX.");
    println!("      Current implementation always uses ALEX (or falls back to binary).");
    println!();
}

// Quick benchmark for range scans
use seerdb::{DB, DBOptions};
use std::time::Instant;
use tempfile::TempDir;

fn main() {
    let temp_dir = TempDir::new().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = temp_dir.path().to_path_buf();
    opts.background_compaction = false;

    let db = DB::open(opts).unwrap();

    // Write 10K entries (reduced for faster testing)
    println!("Writing 10K entries...");
    for i in 0..10_000 {
        let key = format!("key{:06}", i);
        let value = "value".repeat(100); // 500 bytes
        db.put(key.as_bytes(), value.as_bytes()).unwrap();
    }
    
    // Flush to SSTables
    db.flush().unwrap();
    println!("Flushed to SSTables");

    // Benchmark range scans
    let num_scans = 1000;
    println!("\nRunning {} range scans (100 keys each)...", num_scans);
    
    let start = Instant::now();
    for i in 0..num_scans {
        let start_key = format!("key{:06}", i * 10);
        let end_key = format!("key{:06}", i * 10 + 100);
        
        let count = db.range(start_key.as_bytes(), Some(end_key.as_bytes()))
            .unwrap()
            .count();
        
        if i == 0 {
            println!("First scan returned {} keys", count);
        }
    }
    let elapsed = start.elapsed();
    
    let scans_per_sec = num_scans as f64 / elapsed.as_secs_f64();
    let us_per_scan = elapsed.as_micros() as f64 / num_scans as f64;
    
    println!("\nResults:");
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());
    println!("  Scans/sec: {:.0}", scans_per_sec);
    println!("  Time per scan: {:.2} µs", us_per_scan);
    println!("  Time per scan: {:.2} ms", us_per_scan / 1000.0);
}

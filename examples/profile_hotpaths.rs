// Profile hot paths to identify SIMD optimization candidates
// Measures time spent in: bloom filters, key comparison, data access

use seerdb::{BloomFilter, DBOptions, DB};
use std::time::Instant;
use bytes::Bytes;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Hot Path Profiling ===\n");

    // Test 1: Bloom filter performance
    println!("1. Bloom Filter Performance");
    println!("   Testing contains() on 100k keys (hot path for reads)");

    let mut bloom = BloomFilter::new(100_000, 0.01);

    // Insert keys
    let start = Instant::now();
    for i in 0..100_000 {
        let key = format!("key_{:010}", i);
        bloom.insert(&key);
    }
    let insert_time = start.elapsed();

    // Positive lookups (keys in set)
    let start = Instant::now();
    for i in 0..100_000 {
        let key = format!("key_{:010}", i);
        assert!(bloom.contains(&key));
    }
    let positive_lookup_time = start.elapsed();

    // Negative lookups (keys NOT in set - triggers all hash checks)
    let start = Instant::now();
    for i in 1_000_000..1_100_000 {
        let key = format!("key_{:010}", i);
        let _ = bloom.contains(&key);
    }
    let negative_lookup_time = start.elapsed();

    println!("   Insert time:          {:?} ({:.0} ops/sec)",
        insert_time, 100_000.0 / insert_time.as_secs_f64());
    println!("   Positive lookup time: {:?} ({:.0} ops/sec)",
        positive_lookup_time, 100_000.0 / positive_lookup_time.as_secs_f64());
    println!("   Negative lookup time: {:?} ({:.0} ops/sec)",
        negative_lookup_time, 100_000.0 / negative_lookup_time.as_secs_f64());

    // Calculate time per hash operation
    // Each lookup does num_hashes operations (typically 7 for 1% FPR)
    let ops_per_lookup = 7; // Approximate for 1% FPR
    let hash_ops = 100_000 * ops_per_lookup;
    let time_per_hash = negative_lookup_time.as_nanos() as f64 / hash_ops as f64;
    println!("   Time per hash op:     {:.1} ns", time_per_hash);
    println!("   ^ SIMD target: Could 4x this with vectorized bit checks\n");

    // Test 2: Key comparison performance (binary search in SSTable)
    println!("2. Key Comparison Performance");
    println!("   Testing binary search on 100k sorted keys");

    let keys: Vec<String> = (0..100_000)
        .map(|i| format!("key_{:010}", i))
        .collect();

    let start = Instant::now();
    for i in 0..100_000 {
        let target = format!("key_{:010}", i);
        let _ = keys.binary_search(&target);
    }
    let search_time = start.elapsed();

    println!("   Binary search time:   {:?} ({:.0} searches/sec)",
        search_time, 100_000.0 / search_time.as_secs_f64());

    // Estimate number of comparisons (log2(n) per search)
    let comparisons = 100_000 * 17; // log2(100k) ≈ 17
    let time_per_cmp = search_time.as_nanos() as f64 / comparisons as f64;
    println!("   Time per comparison:  {:.1} ns", time_per_cmp);
    println!("   ^ SIMD target: Could vectorize string comparisons\n");

    // Test 3: Database read performance (full stack)
    println!("3. Database Read Performance (Full Stack)");
    println!("   Testing 10k reads from DB with 50k keys");

    let dir = tempfile::tempdir()?;
    let opts = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 64 * 1024 * 1024,
        ..Default::default()
    };
    let db = DB::open(opts)?;

    // Insert data
    let start = Instant::now();
    for i in 0..50_000 {
        let key = format!("key_{:010}", i);
        let value = format!("value_{}", i);
        db.put(key.as_bytes(), value.as_bytes())?;
    }
    let write_time = start.elapsed();
    println!("   Write time:           {:?} ({:.0} ops/sec)",
        write_time, 50_000.0 / write_time.as_secs_f64());

    // Force flush to create SSTables
    db.flush()?;

    // Read all keys (tests bloom filter + binary search + data access)
    let start = Instant::now();
    for i in 0..10_000 {
        let key = format!("key_{:010}", i);
        let value = db.get(key.as_bytes())?;
        assert!(value.is_some());
    }
    let read_time = start.elapsed();
    println!("   Read time:            {:?} ({:.0} ops/sec)",
        read_time, 10_000.0 / read_time.as_secs_f64());

    let time_per_read = read_time.as_micros() as f64 / 10_000.0;
    println!("   Time per read:        {:.1} µs", time_per_read);

    // Negative lookups (all bloom filter checks, no disk I/O)
    let start = Instant::now();
    for i in 1_000_000..1_010_000 {
        let key = format!("key_{:010}", i);
        let _ = db.get(key.as_bytes())?;
    }
    let negative_read_time = start.elapsed();
    println!("   Negative read time:   {:?} ({:.0} ops/sec)",
        negative_read_time, 10_000.0 / negative_read_time.as_secs_f64());

    let time_per_negative = negative_read_time.as_micros() as f64 / 10_000.0;
    println!("   Time per negative:    {:.1} µs", time_per_negative);
    println!("   ^ This is pure bloom filter overhead\n");

    println!("=== SIMD Optimization Candidates ===");
    println!("1. Bloom filter bit checks: {:.1} ns/hash (could 2-4x with SIMD)", time_per_hash);
    println!("2. Key comparisons: {:.1} ns/cmp (moderate gain with SIMD)", time_per_cmp);
    println!("3. Negative lookups: {:.1} µs (bloom filter dominated)", time_per_negative);
    println!("\nRecommendation: Profile with cargo-flamegraph to confirm hotspots");

    Ok(())
}

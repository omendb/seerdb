// Benchmark: SIMD bloom filter vs standard bloom filter
// Tests whether SIMD optimizations provide measurable speedup

use seerdb::{BloomFilter, SimdBloomFilter};
use std::time::Instant;

fn main() {
    println!("=== Bloom Filter: Standard vs SIMD ===\n");

    let num_keys = 100_000;
    let fpr = 0.01;

    println!("Dataset: {} keys, {} target FPR\n", num_keys, fpr);

    // Generate test keys
    let keys: Vec<String> = (0..num_keys)
        .map(|i| format!("key_{:010}", i))
        .collect();

    let negative_keys: Vec<String> = (num_keys..num_keys + 100_000)
        .map(|i| format!("key_{:010}", i))
        .collect();

    // ========================================
    // Standard Bloom Filter
    // ========================================
    println!("1. Standard Bloom Filter (bitpacked)");

    // Insert
    let mut standard_bloom = BloomFilter::new(num_keys, fpr);
    let start = Instant::now();
    for key in &keys {
        standard_bloom.insert(key);
    }
    let insert_time = start.elapsed();
    println!("   Insert time:      {:?} ({:.0} ops/sec)",
        insert_time, num_keys as f64 / insert_time.as_secs_f64());

    // Positive lookups
    let start = Instant::now();
    for key in &keys {
        assert!(standard_bloom.contains(key));
    }
    let positive_time = start.elapsed();
    let ns_per_lookup_pos = positive_time.as_nanos() as f64 / num_keys as f64;
    println!("   Positive lookup:  {:?} ({:.1} ns/op)", positive_time, ns_per_lookup_pos);

    // Negative lookups
    let start = Instant::now();
    for key in &negative_keys {
        let _ = standard_bloom.contains(key);
    }
    let negative_time = start.elapsed();
    let ns_per_lookup_neg = negative_time.as_nanos() as f64 / negative_keys.len() as f64;
    println!("   Negative lookup:  {:?} ({:.1} ns/op)", negative_time, ns_per_lookup_neg);

    println!("   Memory:           {} bytes\n", standard_bloom.size_bytes());

    // ========================================
    // SIMD Bloom Filter
    // ========================================
    println!("2. SIMD Bloom Filter");

    // Insert
    let mut simd_bloom = SimdBloomFilter::new(num_keys, fpr);
    let start = Instant::now();
    for key in &keys {
        simd_bloom.insert(key);
    }
    let simd_insert_time = start.elapsed();
    println!("   Insert time:      {:?} ({:.0} ops/sec)",
        simd_insert_time, num_keys as f64 / simd_insert_time.as_secs_f64());

    // Positive lookups
    let start = Instant::now();
    for key in &keys {
        assert!(simd_bloom.contains(key));
    }
    let simd_positive_time = start.elapsed();
    let simd_ns_per_lookup_pos = simd_positive_time.as_nanos() as f64 / num_keys as f64;
    println!("   Positive lookup:  {:?} ({:.1} ns/op)", simd_positive_time, simd_ns_per_lookup_pos);

    // Negative lookups
    let start = Instant::now();
    for key in &negative_keys {
        let _ = simd_bloom.contains(key);
    }
    let simd_negative_time = start.elapsed();
    let simd_ns_per_lookup_neg = simd_negative_time.as_nanos() as f64 / negative_keys.len() as f64;
    println!("   Negative lookup:  {:?} ({:.1} ns/op)", simd_negative_time, simd_ns_per_lookup_neg);

    println!("   Memory:           {} bytes\n", simd_bloom.size_bytes());

    // ========================================
    // Comparison
    // ========================================
    println!("=== Results ===");
    println!("Standard positive lookups: {:.1} ns/op", ns_per_lookup_pos);
    println!("SIMD positive lookups:     {:.1} ns/op", simd_ns_per_lookup_pos);

    if simd_positive_time < positive_time {
        let speedup = positive_time.as_nanos() as f64 / simd_positive_time.as_nanos() as f64;
        println!("SIMD speedup (positive):   {:.2}x faster", speedup);
    } else {
        let slowdown = simd_positive_time.as_nanos() as f64 / positive_time.as_nanos() as f64;
        println!("SIMD slowdown (positive):  {:.2}x slower", slowdown);
    }

    println!("\nStandard negative lookups: {:.1} ns/op", ns_per_lookup_neg);
    println!("SIMD negative lookups:     {:.1} ns/op", simd_ns_per_lookup_neg);

    if simd_negative_time < negative_time {
        let speedup = negative_time.as_nanos() as f64 / simd_negative_time.as_nanos() as f64;
        println!("SIMD speedup (negative):   {:.2}x faster", speedup);
        println!("\n✅ SIMD wins! Consider using SimdBloomFilter.");
    } else {
        let slowdown = simd_negative_time.as_nanos() as f64 / negative_time.as_nanos() as f64;
        println!("SIMD slowdown (negative):  {:.2}x slower", slowdown);
        println!("\n❌ Standard faster. Keep current BloomFilter.");
    }

    // Check false positive rate
    let mut false_positives = 0;
    for key in &negative_keys {
        if simd_bloom.contains(key) {
            false_positives += 1;
        }
    }
    let actual_fpr = false_positives as f64 / negative_keys.len() as f64;
    println!("\nFalse positive rate: {:.3}% (target: {:.1}%)", actual_fpr * 100.0, fpr * 100.0);
}

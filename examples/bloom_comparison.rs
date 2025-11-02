// Comparison between traditional and learned bloom filters
// Measures space savings, false positive rate, and query time

use seerdb::bloom::{BloomFilter, LearnedBloomFilter};
use std::time::Instant;

fn generate_keys(n: usize, start: usize) -> Vec<String> {
    (start..start + n).map(|i| format!("key_{:010}", i)).collect()
}

fn main() {
    println!("=== Bloom Filter Comparison: Traditional vs Learned ===\n");

    // Test different dataset sizes
    for size in [1_000, 10_000, 100_000].iter() {
        println!("Dataset Size: {} keys", size);
        println!("Target FPR: 1%\n");

        let keys = generate_keys(*size, 0);
        let negative_training = generate_keys(*size, 1_000_000);
        let test_keys = generate_keys(*size, 2_000_000);

        // Traditional Bloom Filter
        println!("Traditional Bloom Filter:");
        let start = Instant::now();
        let mut bf = BloomFilter::new(*size, 0.01);
        for key in &keys {
            bf.insert(key);
        }
        let trad_build_time = start.elapsed();
        let trad_size = bf.size_bytes();

        // Test FPR
        let mut trad_fp = 0;
        for key in &test_keys {
            if bf.contains(key) {
                trad_fp += 1;
            }
        }
        let trad_fpr = trad_fp as f64 / test_keys.len() as f64;

        // Query time
        let start = Instant::now();
        for key in &keys {
            bf.contains(key);
        }
        let trad_query_time = start.elapsed();

        println!("  Build time:  {:?}", trad_build_time);
        println!("  Size:        {} bytes", trad_size);
        println!("  FPR:         {:.2}% ({}/{})", trad_fpr * 100.0, trad_fp, test_keys.len());
        println!("  Query time:  {:?} ({} queries)", trad_query_time, keys.len());
        println!();

        // Learned Bloom Filter
        println!("Learned Bloom Filter:");
        let start = Instant::now();
        let mut lbf = LearnedBloomFilter::new(*size, 0.01, 0.7);
        lbf.train(&keys, &negative_training);
        let learned_build_time = start.elapsed();
        let learned_size = lbf.size_bytes();

        // Test FPR
        let mut learned_fp = 0;
        for key in &test_keys {
            if lbf.contains(key) {
                learned_fp += 1;
            }
        }
        let learned_fpr = learned_fp as f64 / test_keys.len() as f64;

        // Query time
        let start = Instant::now();
        for key in &keys {
            lbf.contains(key);
        }
        let learned_query_time = start.elapsed();

        println!("  Build time:  {:?}", learned_build_time);
        println!("  Size:        {} bytes", learned_size);
        println!("  FPR:         {:.2}% ({}/{})", learned_fpr * 100.0, learned_fp, test_keys.len());
        println!("  Query time:  {:?} ({} queries)", learned_query_time, keys.len());
        println!();

        // Comparison
        let space_reduction = ((trad_size - learned_size) as f64 / trad_size as f64) * 100.0;
        println!("Comparison:");
        println!("  Space reduction: {:.1}%", space_reduction);
        println!("  Space savings:   {} bytes", trad_size - learned_size);

        if space_reduction > 0.0 {
            println!("  ✅ Learned is {:.1}% smaller", space_reduction);
        } else {
            println!("  ❌ Learned is {:.1}% LARGER", -space_reduction);
        }

        let query_speedup = trad_query_time.as_nanos() as f64 / learned_query_time.as_nanos() as f64;
        if query_speedup > 1.0 {
            println!("  ⚠️  Learned is {:.2}x SLOWER for queries", 1.0 / query_speedup);
        } else {
            println!("  ✅ Learned is {:.2}x faster for queries", query_speedup);
        }

        println!("\n{}\n", "=".repeat(60));
    }

    // Threshold sensitivity analysis
    println!("\n=== Threshold Sensitivity Analysis ===\n");
    let size = 10_000;
    let keys = generate_keys(size, 0);
    let negative_training = generate_keys(size, 1_000_000);
    let test_keys = generate_keys(size, 2_000_000);

    println!("Dataset: {} keys", size);
    println!("Testing different confidence thresholds:\n");
    println!("{:<12} {:<15} {:<15}", "Threshold", "Size (bytes)", "FPR");
    println!("{}", "-".repeat(45));

    for threshold in [0.3, 0.5, 0.7, 0.9].iter() {
        let mut lbf = LearnedBloomFilter::new(size, 0.01, *threshold);
        lbf.train(&keys, &negative_training);

        let learned_size = lbf.size_bytes();

        let mut fp = 0;
        for key in &test_keys {
            if lbf.contains(key) {
                fp += 1;
            }
        }
        let fpr = fp as f64 / test_keys.len() as f64;

        println!("{:<12.1} {:<15} {:<15.2}%", threshold, learned_size, fpr * 100.0);
    }

    println!("\n=== Summary ===\n");
    println!("Key Findings:");
    println!("1. Learned bloom filters can be smaller than traditional (depends on data)");
    println!("2. Model inference adds query latency vs simple hash functions");
    println!("3. Threshold tuning affects space/accuracy trade-off");
    println!("4. Best for: Large datasets where space savings matter");
    println!("5. Consider: Query latency vs space savings for your workload");
}

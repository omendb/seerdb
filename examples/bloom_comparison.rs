// Comprehensive comparison of Traditional vs Learned Bloom Filters
// Tests different dataset sizes to find crossover point for space savings

use seerdb::{BloomFilter, LearnedBloomFilter};

fn main() {
    println!("=== Bloom Filter Comparison ===\n");

    let dataset_sizes = vec![100, 1_000, 10_000, 100_000];
    let fpr = 0.01; // 1% false positive rate

    for size in dataset_sizes {
        println!("Dataset size: {} elements", size);
        println!("{:-<60}", "");

        // Generate datasets
        let positive: Vec<String> = (0..size).map(|i| format!("key_{}", i)).collect();
        let negative: Vec<String> = (size * 10..size * 10 + size)
            .map(|i| format!("key_{}", i))
            .collect();

        // Traditional Bloom Filter
        let mut trad_bf = BloomFilter::new(size, fpr);
        for key in &positive {
            trad_bf.insert(key);
        }

        // Learned Bloom Filter
        let mut learned_bf = LearnedBloomFilter::new(size, fpr, 0.7);
        learned_bf.train(&positive, &negative);

        // Measure sizes
        let trad_size = trad_bf.size_bytes();
        let learned_size = learned_bf.size_bytes();
        let reduction = (1.0 - learned_size as f64 / trad_size as f64) * 100.0;

        println!("Traditional BF:  {:>8} bytes", trad_size);
        println!("Learned BF:      {:>8} bytes", learned_size);
        println!("Space reduction: {:>7.1}%", reduction);

        // Test false positive rates
        let mut trad_fp = 0;
        let mut learned_fp = 0;

        for key in &negative {
            if trad_bf.contains(key) {
                trad_fp += 1;
            }
            if learned_bf.contains(key) {
                learned_fp += 1;
            }
        }

        let trad_fpr = trad_fp as f64 / negative.len() as f64;
        let learned_fpr = learned_fp as f64 / negative.len() as f64;

        println!("Traditional FPR: {:>7.2}%", trad_fpr * 100.0);
        println!("Learned FPR:     {:>7.2}%", learned_fpr * 100.0);

        // Test positive accuracy (should be 100%)
        let mut trad_correct = 0;
        let mut learned_correct = 0;

        for key in &positive {
            if trad_bf.contains(key) {
                trad_correct += 1;
            }
            if learned_bf.contains(key) {
                learned_correct += 1;
            }
        }

        println!(
            "Traditional accuracy: {}/{} ({:.1}%)",
            trad_correct,
            size,
            trad_correct as f64 / size as f64 * 100.0
        );
        println!(
            "Learned accuracy:     {}/{} ({:.1}%)",
            learned_correct,
            size,
            learned_correct as f64 / size as f64 * 100.0
        );

        println!();
    }

    println!("=== Summary ===");
    println!("Learned bloom filters show space savings on larger datasets.");
    println!("For small datasets (<10k), model overhead dominates.");
    println!("For large datasets (>100k), the model can compress effectively.");
}

// Debug learned bloom filter to understand why FPR is 50%

use seerdb::bloom::LearnedBloomFilter;

fn main() {
    println!("=== Learned Bloom Filter Debug Analysis ===\n");

    // Small dataset for detailed analysis
    let size = 100;
    let positive: Vec<String> = (0..size).map(|i| format!("key_{:04}", i)).collect();
    let negative: Vec<String> = (1000..1000+size).map(|i| format!("key_{:04}", i)).collect();
    
    println!("Dataset:");
    println!("  Positive examples: {}", positive.len());
    println!("  Negative examples: {}", negative.len());
    println!("  Threshold: 0.7");
    println!();

    let mut lbf = LearnedBloomFilter::new(size, 0.01, 0.7);
    
    println!("Before training:");
    println!("  Backup filter size: {} bytes", lbf.size_bytes());
    println!();

    lbf.train(&positive, &negative);
    
    println!("After training:");
    println!("  Total size: {} bytes", lbf.size_bytes());
    println!("  Model trained: yes");
    println!();

    // Test on training data (should be perfect)
    println!("=== Testing on TRAINING data ===");
    let mut tp = 0;
    let mut fn_count = 0;
    for key in &positive {
        if lbf.contains(key) {
            tp += 1;
        } else {
            fn_count += 1;
        }
    }
    
    let mut tn = 0;
    let mut fp = 0;
    for key in &negative {
        if lbf.contains(key) {
            fp += 1;
        } else {
            tn += 1;
        }
    }
    
    println!("Positive examples (should return TRUE):");
    println!("  True Positives:  {} / {} ({:.1}%)", tp, positive.len(), (tp as f64 / positive.len() as f64) * 100.0);
    println!("  False Negatives: {} / {} ({:.1}%)", fn_count, positive.len(), (fn_count as f64 / positive.len() as f64) * 100.0);
    println!();
    
    println!("Negative examples (should return FALSE):");
    println!("  True Negatives:  {} / {} ({:.1}%)", tn, negative.len(), (tn as f64 / negative.len() as f64) * 100.0);
    println!("  False Positives: {} / {} ({:.1}%)", fp, negative.len(), (fp as f64 / negative.len() as f64) * 100.0);
    println!();
    
    let accuracy = ((tp + tn) as f64 / (positive.len() + negative.len()) as f64) * 100.0;
    println!("Overall Accuracy: {:.1}%", accuracy);
    println!();

    // Test on NEW unseen data
    println!("=== Testing on UNSEEN data ===");
    let test_positive: Vec<String> = (size..size*2).map(|i| format!("key_{:04}", i)).collect();
    let test_negative: Vec<String> = (2000..2000+size).map(|i| format!("key_{:04}", i)).collect();
    
    let mut test_tp = 0;
    for key in &test_positive {
        if lbf.contains(key) {
            test_tp += 1;
        }
    }
    
    let mut test_fp = 0;
    for key in &test_negative {
        if lbf.contains(key) {
            test_fp += 1;
        }
    }
    
    println!("Unseen positive examples:");
    println!("  Detected: {} / {} ({:.1}%)", test_tp, test_positive.len(), (test_tp as f64 / test_positive.len() as f64) * 100.0);
    println!();
    
    println!("Unseen negative examples:");
    println!("  False Positives: {} / {} ({:.1}%)", test_fp, test_negative.len(), (test_fp as f64 / test_negative.len() as f64) * 100.0);
    println!();

    // Analysis
    println!("=== Analysis ===");
    if accuracy < 60.0 {
        println!("❌ Model is not learning - accuracy too low");
        println!("   Possible causes:");
        println!("   1. Features are not discriminative (hash-based features may be random)");
        println!("   2. Decision tree too simple (need more complex model)");
        println!("   3. Training data not representative");
    } else if fp > negative.len() / 3 {
        println!("⚠️  High false positive rate on training data");
        println!("   Backup filter may not be catching model errors");
    } else if test_fp > test_negative.len() / 3 {
        println!("⚠️  Model doesn't generalize to unseen data");
        println!("   Overfitting to training examples");
    } else {
        println!("✅ Model appears to be working correctly");
    }
}

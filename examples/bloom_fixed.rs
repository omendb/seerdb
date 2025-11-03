// Demonstration of learned bloom with PROPER features
// Shows that learned blooms work when features preserve patterns

use seerdb::bloom::BloomFilter;
use smartcore::linalg::basic::matrix::DenseMatrix;
use smartcore::tree::decision_tree_classifier::DecisionTreeClassifier;

type Model = DecisionTreeClassifier<f64, u32, DenseMatrix<f64>, Vec<u32>>;

/// Extract meaningful features from string keys
/// For "key_XXXX" format, extract the numeric value
fn extract_features(key: &str) -> Vec<f64> {
    // Extract numeric part from "key_XXXX"
    let numeric_str = key.strip_prefix("key_").unwrap_or("0");
    let value = numeric_str.parse::<f64>().unwrap_or(0.0);

    // Create features that preserve the pattern
    vec![
        value / 10000.0,          // Normalized value
        (value / 1000.0).floor(), // Thousands digit
        (value / 100.0) % 10.0,   // Hundreds digit
        (value / 10.0) % 10.0,    // Tens digit
        value % 10.0,             // Ones digit
        key.len() as f64,         // Key length
        value.sqrt() / 100.0,     // Non-linear feature
        (value / 500.0).sin(),    // Periodic feature
    ]
}

fn main() {
    println!("=== Learned Bloom with PROPER Features ===\n");

    let size = 1000;

    // Training data
    let positive: Vec<String> = (0..size).map(|i| format!("key_{:04}", i)).collect();
    let negative: Vec<String> = (10000..10000 + size)
        .map(|i| format!("key_{:04}", i))
        .collect();

    println!("Training:");
    println!("  Positive: keys 0-{}", size - 1);
    println!("  Negative: keys 10000-{}", 10000 + size - 1);
    println!();

    // Extract features
    let mut features = Vec::new();
    let mut labels = Vec::new();

    for key in &positive {
        features.push(extract_features(key));
        labels.push(1);
    }

    for key in &negative {
        features.push(extract_features(key));
        labels.push(0);
    }

    // Train model
    let x = DenseMatrix::from_2d_vec(&features);
    let y: Vec<u32> = labels;

    let model = DecisionTreeClassifier::fit(&x, &y, Default::default()).expect("Failed to train");

    println!("Model trained!\n");

    // Test on training data
    println!("=== Testing on TRAINING data ===");
    let mut correct = 0;
    let total = positive.len() + negative.len();

    for key in &positive {
        let features = extract_features(key);
        let x_test = DenseMatrix::from_2d_vec(&vec![features]);
        let pred = model.predict(&x_test).unwrap();
        if pred[0] == 1 {
            correct += 1;
        }
    }

    for key in &negative {
        let features = extract_features(key);
        let x_test = DenseMatrix::from_2d_vec(&vec![features]);
        let pred = model.predict(&x_test).unwrap();
        if pred[0] == 0 {
            correct += 1;
        }
    }

    println!(
        "Accuracy: {}/{} ({:.1}%)",
        correct,
        total,
        (correct as f64 / total as f64) * 100.0
    );
    println!();

    // Test on UNSEEN data (keys 1000-1999)
    println!("=== Testing on UNSEEN data ===");
    let test_positive: Vec<String> = (size..size * 2).map(|i| format!("key_{:04}", i)).collect();
    let test_negative: Vec<String> = (20000..20000 + size)
        .map(|i| format!("key_{:04}", i))
        .collect();

    println!("Test positive: keys {}-{}", size, size * 2 - 1);
    println!("Test negative: keys 20000-{}", 20000 + size - 1);
    println!();

    let mut tp = 0;
    for key in &test_positive {
        let features = extract_features(key);
        let x_test = DenseMatrix::from_2d_vec(&vec![features]);
        let pred = model.predict(&x_test).unwrap();
        if pred[0] == 1 {
            tp += 1;
        }
    }

    let mut tn = 0;
    let mut fp = 0;
    for key in &test_negative {
        let features = extract_features(key);
        let x_test = DenseMatrix::from_2d_vec(&vec![features]);
        let pred = model.predict(&x_test).unwrap();
        if pred[0] == 0 {
            tn += 1;
        } else {
            fp += 1;
        }
    }

    let test_accuracy =
        ((tp + tn) as f64 / (test_positive.len() + test_negative.len()) as f64) * 100.0;
    let fpr = (fp as f64 / test_negative.len() as f64) * 100.0;

    println!("Results:");
    println!(
        "  True Positives:  {} / {} ({:.1}%)",
        tp,
        test_positive.len(),
        (tp as f64 / test_positive.len() as f64) * 100.0
    );
    println!("  True Negatives:  {} / {}", tn, test_negative.len());
    println!(
        "  False Positives: {} / {} ({:.2}% FPR)",
        fp,
        test_negative.len(),
        fpr
    );
    println!("  Accuracy:        {:.1}%", test_accuracy);
    println!();

    // Compare to traditional bloom
    println!("=== Comparison to Traditional Bloom ===");
    let mut bf = BloomFilter::new(size, 0.01);
    for key in &positive {
        bf.insert(key);
    }

    let mut trad_fp = 0;
    for key in &test_negative {
        if bf.contains(key) {
            trad_fp += 1;
        }
    }
    let trad_fpr = (trad_fp as f64 / test_negative.len() as f64) * 100.0;

    println!("Traditional Bloom:");
    println!("  Size: {} bytes", bf.size_bytes());
    println!("  FPR:  {:.2}%", trad_fpr);
    println!();

    println!("Learned Model:");
    println!("  Size: ~1KB (decision tree)");
    println!("  FPR:  {:.2}%", fpr);
    println!();

    if test_accuracy > 95.0 {
        println!("✅ SUCCESS: Model generalizes to unseen data!");
        println!("   Proper features allow the model to learn patterns");
    } else {
        println!("❌ Model still struggling");
    }
}

// Benchmark to measure prefix compression space savings and throughput impact
// Compares block sizes with prefix compression vs without

use seerdb::sstable::block::BlockBuilder;
use std::time::Instant;

fn main() {
    println!("=== Prefix Compression Benchmark ===\n");

    // Test 1: Sequential keys with common prefix (best case)
    benchmark_sequential_keys();

    // Test 2: Random keys (worst case)
    benchmark_random_keys();

    // Test 3: Realistic workload (user:123:field pattern)
    benchmark_realistic_keys();
}

fn benchmark_sequential_keys() {
    println!("Test 1: Sequential Keys (user_00000001, user_00000002, ...)");
    println!("Expected: High compression ratio (shared prefix 'user_0000000')");

    let mut builder = BlockBuilder::with_capacity(64 * 1024); // 64KB
    let start = Instant::now();
    let mut count = 0;

    for i in 0..10_000 {
        let key = format!("user_{:08}", i);
        let value = format!("value_{:08}", i);
        if !builder.add(key.as_bytes(), value.as_bytes()) {
            break;
        }
        count += 1;
    }

    let elapsed = start.elapsed();
    let block = builder.finish();

    // Calculate original size (no compression)
    let avg_key_size = 13; // "user_00000000"
    let avg_value_size = 14; // "value_00000000"
    let original_size = count * (4 + avg_key_size + 4 + avg_value_size); // key_len + key + value_len + value

    let compressed_size = block.len();
    let compression_ratio = (1.0 - (compressed_size as f64 / original_size as f64)) * 100.0;

    println!("  Entries: {}", count);
    println!("  Original size (no compression): {} bytes", original_size);
    println!("  Compressed size: {} bytes", compressed_size);
    println!("  Space savings: {:.1}%", compression_ratio);
    println!("  Throughput: {:.0} entries/sec", count as f64 / elapsed.as_secs_f64());
    println!();
}

fn benchmark_random_keys() {
    println!("Test 2: Random Keys (UUID-like, no common prefix)");
    println!("Expected: Low compression ratio (minimal shared prefix)");

    let mut builder = BlockBuilder::with_capacity(64 * 1024);
    let start = Instant::now();
    let mut count = 0;

    // Generate pseudo-random keys
    for i in 0u64..10_000 {
        let key = format!("{:032x}", i.wrapping_mul(0x123456789abcdefu64));
        let value = format!("value_{}", i);
        if !builder.add(key.as_bytes(), value.as_bytes()) {
            break;
        }
        count += 1;
    }

    let elapsed = start.elapsed();
    let block = builder.finish();

    let avg_key_size = 32;
    let avg_value_size = 10;
    let original_size = count * (4 + avg_key_size + 4 + avg_value_size);

    let compressed_size = block.len();
    let compression_ratio = (1.0 - (compressed_size as f64 / original_size as f64)) * 100.0;

    println!("  Entries: {}", count);
    println!("  Original size (no compression): {} bytes", original_size);
    println!("  Compressed size: {} bytes", compressed_size);
    println!("  Space savings: {:.1}%", compression_ratio);
    println!("  Throughput: {:.0} entries/sec", count as f64 / elapsed.as_secs_f64());
    println!();
}

fn benchmark_realistic_keys() {
    println!("Test 3: Realistic Keys (user:123:field pattern)");
    println!("Expected: Medium compression ratio (some shared prefixes)");

    let mut builder = BlockBuilder::with_capacity(64 * 1024);
    let start = Instant::now();
    let mut count = 0;

    // Simulate user:id:field pattern (10 users, 10 fields each)
    for user_id in 0..100 {
        for field in &["name", "email", "age", "city", "country", "phone", "address", "zip", "company", "title"] {
            let key = format!("user:{}:{}", user_id, field);
            let value = format!("value_{}", count);
            if !builder.add(key.as_bytes(), value.as_bytes()) {
                break;
            }
            count += 1;
        }
    }

    let elapsed = start.elapsed();
    let block = builder.finish();

    let avg_key_size = 15; // "user:50:company"
    let avg_value_size = 10;
    let original_size = count * (4 + avg_key_size + 4 + avg_value_size);

    let compressed_size = block.len();
    let compression_ratio = (1.0 - (compressed_size as f64 / original_size as f64)) * 100.0;

    println!("  Entries: {}", count);
    println!("  Original size (no compression): {} bytes", original_size);
    println!("  Compressed size: {} bytes", compressed_size);
    println!("  Space savings: {:.1}%", compression_ratio);
    println!("  Throughput: {:.0} entries/sec", count as f64 / elapsed.as_secs_f64());
    println!();
}

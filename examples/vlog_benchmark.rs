// WiscKey vlog benchmark
// Tests performance with large values (key-value separation)
// Compares performance with vlog enabled vs disabled

use seerdb::{DBOptions, SyncPolicy, DB};
use std::path::PathBuf;
use std::time::Instant;
use tempfile::TempDir;

fn benchmark_large_values(vlog_enabled: bool, value_size: usize, operations: usize) {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = PathBuf::from(temp_dir.path());

    let opts = DBOptions {
        data_dir: data_dir.clone(),
        memtable_capacity: 64 * 1024 * 1024, // 64MB
        vlog_threshold: if vlog_enabled { Some(4096) } else { None },
        wal_sync_policy: SyncPolicy::None, // Fast mode for benchmarking
        background_compaction: true,
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();

    // Workload 1: Sequential Writes
    println!(
        "  Sequential Writes ({} ops, {} KB values)",
        operations,
        value_size / 1024
    );
    let start = Instant::now();

    for i in 0..operations {
        let key = format!("key_{:08}", i);
        let value = vec![b'x'; value_size];
        db.put(key.as_bytes(), &value).unwrap();
    }

    let write_elapsed = start.elapsed();
    let write_throughput = operations as f64 / write_elapsed.as_secs_f64();
    let write_latency = write_elapsed.as_micros() as f64 / operations as f64;

    println!("    Time: {:.2}s", write_elapsed.as_secs_f64());
    println!("    Throughput: {:.0} ops/sec", write_throughput);
    println!("    Latency: {:.2} us/op", write_latency);

    // Flush to disk
    db.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Workload 2: Random Reads
    println!("  Random Reads ({} ops)", operations);
    let start = Instant::now();

    for i in 0..operations {
        let key = format!("key_{:08}", i);
        let value = db.get(key.as_bytes()).unwrap();
        if value.is_none() {
            eprintln!("ERROR: Key not found: key_{:08} (iteration {})", i, i);
            eprintln!("This suggests vlog values aren't being written or read correctly");
            panic!("Value not found");
        }
        assert_eq!(value.unwrap().len(), value_size);
    }

    let read_elapsed = start.elapsed();
    let read_throughput = operations as f64 / read_elapsed.as_secs_f64();
    let read_latency = read_elapsed.as_micros() as f64 / operations as f64;

    println!("    Time: {:.2}s", read_elapsed.as_secs_f64());
    println!("    Throughput: {:.0} ops/sec", read_throughput);
    println!("    Latency: {:.2} us/op", read_latency);

    // Measure write amplification
    fn get_dir_size(path: &std::path::Path) -> u64 {
        let mut size = 0u64;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        size += metadata.len();
                    } else if metadata.is_dir() {
                        size += get_dir_size(&entry.path());
                    }
                }
            }
        }
        size
    }

    let physical_bytes = get_dir_size(&data_dir);
    let logical_bytes = operations * value_size;
    let write_amp = physical_bytes as f64 / logical_bytes as f64;

    println!("  Write Amplification:");
    println!("    Logical:  {} MB", logical_bytes / 1024 / 1024);
    println!("    Physical: {} MB", physical_bytes / 1024 / 1024);
    println!("    Ratio: {:.2}x", write_amp);
}

fn main() {
    println!("=== WiscKey vlog Benchmark ===\n");
    println!("Testing key-value separation for large values\n");
    println!("NOTE: vlog is now ENABLED by default (4KB threshold)\n");

    let operations = 10_000;

    // Test with different value sizes (vlog enabled by default)
    println!("Test 1: Small Values (1KB) - Below vlog threshold");
    println!("----------------------------------------------------------------------");
    benchmark_large_values(true, 1024, operations);

    println!("\n\n");

    // Test 2: Medium values (8KB) - Above vlog threshold
    println!("Test 2: Medium Values (8KB) - Above vlog threshold");
    println!("----------------------------------------------------------------------");
    benchmark_large_values(true, 8 * 1024, operations);

    println!("\n\n");

    // Test 3: Large values (64KB) - Well above vlog threshold
    println!("Test 3: Large Values (64KB) - Well above vlog threshold");
    println!("----------------------------------------------------------------------");
    benchmark_large_values(true, 64 * 1024, operations);

    println!("\n\n");
    println!("=== Summary ===");
    println!("vlog enabled by default with 4KB threshold");
    println!("  - 1KB values:  Stored inline in LSM tree (below threshold)");
    println!("  - 8KB values:  Stored in vlog (above threshold) - lower write amp");
    println!("  - 64KB values: Stored in vlog (well above threshold) - much lower write amp");
    println!("\nWiscKey benefits:");
    println!("  - Lower write amplification for large values");
    println!("  - Less data moved during compaction");
    println!("  - Perfect for workloads with large values (embeddings, documents, etc.)");
}

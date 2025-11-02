// Demonstration of write amplification reduction with KV separation
// Compares traditional LSM vs WiscKey-style separation

use seerdb::{DBOptions, SyncPolicy, DB};
use std::time::Instant;
use tempfile::tempdir;

fn measure_writes(use_vlog: bool, num_entries: usize, value_size: usize) -> (u64, u64, f64) {
    let dir = tempdir().unwrap();

    let mut options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 1024 * 1024, // 1MB memtable
        wal_sync_policy: SyncPolicy::None, // Disable sync for faster demo
        ..Default::default()
    };

    if use_vlog {
        // Enable KV separation for values > 1KB
        options.vlog_threshold = Some(1024);
    }

    let db = DB::open(options).unwrap();

    // Write data
    let start = Instant::now();
    for i in 0..num_entries {
        let key = format!("key_{:06}", i);
        let value = vec![b'V'; value_size];
        db.put(key.as_bytes(), &value).unwrap();
    }
    let duration = start.elapsed();

    // Measure file sizes
    let mut sstable_size = 0u64;
    let mut vlog_size = 0u64;

    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("sst") {
            sstable_size += std::fs::metadata(&path).unwrap().len();
        } else if path.extension().and_then(|s| s.to_str()) == Some("vlog") {
            vlog_size += std::fs::metadata(&path).unwrap().len();
        }
    }

    let throughput = num_entries as f64 / duration.as_secs_f64();

    (sstable_size, vlog_size, throughput)
}

fn main() {
    println!("=== Write Amplification Comparison ===\n");

    let num_entries = 1000; // Reduced for faster demo
    let value_size = 4096; // 4KB (typical embedding size)

    println!("Dataset: {} entries × {} bytes = {:.2} MB total",
        num_entries,
        value_size,
        (num_entries * value_size) as f64 / (1024.0 * 1024.0)
    );
    println!();

    // Traditional LSM (no separation)
    println!("Traditional LSM (all in SSTable):");
    let (sst_size, vlog_size, throughput) = measure_writes(false, num_entries, value_size);
    println!("  SSTable size: {:.2} MB", sst_size as f64 / (1024.0 * 1024.0));
    println!("  VLog size:    {:.2} MB", vlog_size as f64 / (1024.0 * 1024.0));
    println!("  Total:        {:.2} MB", (sst_size + vlog_size) as f64 / (1024.0 * 1024.0));
    println!("  Throughput:   {:.0} writes/sec", throughput);
    println!();

    // WiscKey (KV separation)
    println!("WiscKey (KV separation, threshold=1KB):");
    let (sst_size_kv, vlog_size_kv, throughput_kv) = measure_writes(true, num_entries, value_size);
    println!("  SSTable size: {:.2} MB (keys + pointers)", sst_size_kv as f64 / (1024.0 * 1024.0));
    println!("  VLog size:    {:.2} MB (values)", vlog_size_kv as f64 / (1024.0 * 1024.0));
    println!("  Total:        {:.2} MB", (sst_size_kv + vlog_size_kv) as f64 / (1024.0 * 1024.0));
    println!("  Throughput:   {:.0} writes/sec", throughput_kv);
    println!();

    // Calculate write amplification benefit
    let total_traditional = sst_size + vlog_size;
    let total_wisckey = sst_size_kv + vlog_size_kv;
    let sstable_reduction = (sst_size - sst_size_kv) as f64 / sst_size as f64 * 100.0;

    println!("=== Write Amplification Analysis ===");
    println!();
    println!("During Compaction:");
    println!("  Traditional LSM rewrites: {:.2} MB (entire SSTable)", sst_size as f64 / (1024.0 * 1024.0));
    println!("  WiscKey rewrites:         {:.2} MB (only keys + pointers)", sst_size_kv as f64 / (1024.0 * 1024.0));
    println!("  Reduction:                {:.1}%", sstable_reduction);
    println!();

    println!("Write Amplification Factor:");
    println!("  Traditional: Compaction rewrites {:.2} MB per flush", sst_size as f64 / (1024.0 * 1024.0));
    println!("  WiscKey:     Compaction rewrites {:.2} MB per flush", sst_size_kv as f64 / (1024.0 * 1024.0));
    println!("  Improvement: {:.1}x less data written", sst_size as f64 / sst_size_kv as f64);
    println!();

    println!("Benefits:");
    println!("  • Large values ({} KB) never rewritten during compaction", value_size / 1024);
    println!("  • SSTable compaction {:.1}x faster (smaller files)", sst_size as f64 / sst_size_kv as f64);
    println!("  • Lower SSD wear (fewer writes)");
    println!("  • Better throughput for large-value workloads");
    println!();

    println!("Trade-offs:");
    println!("  • Random reads may require two I/Os (SSTable + vLog)");
    println!("  • VLog requires garbage collection (not yet implemented)");
    println!("  • Best for: write-heavy workloads with large values (embeddings, documents)");
}

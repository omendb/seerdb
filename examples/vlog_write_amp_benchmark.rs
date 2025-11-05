// Write Amplification: Inline vs vlog (WiscKey)
// Tests if vlog achieves 5-10x write amp reduction for large values

use seerdb::{DBOptions, SyncPolicy, DB};
use std::path::PathBuf;
use std::time::Instant;
use tempfile::TempDir;

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

fn run_benchmark(name: &str, vlog_threshold: Option<usize>, value_size: usize, operations: usize) -> (f64, f64) {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("{}", name);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let logical_bytes = operations * value_size;
    println!("Operations:    {}", operations);
    println!("Value size:    {} bytes", value_size);
    println!("Logical data:  {} MB", logical_bytes / 1024 / 1024);
    if let Some(threshold) = vlog_threshold {
        println!("vLog threshold: {} bytes", threshold);
    } else {
        println!("vLog:          DISABLED (all inline)");
    }
    println!();

    let temp_dir = TempDir::new().unwrap();
    let data_dir = PathBuf::from(temp_dir.path());

    let opts = DBOptions {
        data_dir: data_dir.clone(),
        memtable_capacity: 64 * 1024 * 1024, // 64MB
        background_compaction: true,
        wal_sync_policy: SyncPolicy::None,
        vlog_threshold,
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();

    println!("Writing {} operations...", operations);
    let start = Instant::now();

    for i in 0..operations {
        let key = format!("key_{:08}", i);
        let value = vec![b'x'; value_size];
        db.put(key.as_bytes(), &value).unwrap();

        if i % 50_000 == 0 && i > 0 {
            println!("  {} ops written", i);
        }
    }

    db.flush().unwrap();
    let write_time = start.elapsed();

    // Wait for compaction to settle
    println!("Waiting for compaction...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    let physical_bytes = get_dir_size(&data_dir);
    let write_amp = physical_bytes as f64 / logical_bytes as f64;

    println!("\nResults:");
    println!("  Logical data:       {} MB", logical_bytes / 1024 / 1024);
    println!("  Physical data:      {} MB", physical_bytes / 1024 / 1024);
    println!("  Write amplification: {:.2}x", write_amp);
    println!("  Write time:         {:.2}s", write_time.as_secs_f64());
    println!("  Throughput:         {:.0} ops/sec", operations as f64 / write_time.as_secs_f64());

    (write_amp, write_time.as_secs_f64())
}

fn main() {
    println!("╔═══════════════════════════════════════════════════════╗");
    println!("║   Write Amplification: Inline vs vLog (WiscKey)      ║");
    println!("║   Testing if vlog achieves 5-10x write amp reduction ║");
    println!("╚═══════════════════════════════════════════════════════╝");

    let operations = 100_000;

    // Test 1: Small values (1KB) - Should be inline
    println!("\n\n═══════════════════════════════════════");
    println!("Test 1: Small Values (1KB, inline)");
    println!("═══════════════════════════════════════");
    let (small_inline_amp, _) = run_benchmark(
        "Small Values (1KB, inline)",
        None, // No vlog
        1024,
        operations,
    );

    // Test 2: Medium values (8KB) - Inline mode
    println!("\n\n═══════════════════════════════════════");
    println!("Test 2: Medium Values (8KB, inline)");
    println!("═══════════════════════════════════════");
    let (medium_inline_amp, _) = run_benchmark(
        "Medium Values (8KB, inline)",
        None, // No vlog
        8 * 1024,
        operations,
    );

    // Test 3: Medium values (8KB) - vlog mode
    println!("\n\n═══════════════════════════════════════");
    println!("Test 3: Medium Values (8KB, vlog)");
    println!("═══════════════════════════════════════");
    let (medium_vlog_amp, _) = run_benchmark(
        "Medium Values (8KB, vlog)",
        Some(4096), // 4KB threshold
        8 * 1024,
        operations,
    );

    // Test 4: Large values (64KB) - Inline mode
    println!("\n\n═══════════════════════════════════════");
    println!("Test 4: Large Values (64KB, inline)");
    println!("═══════════════════════════════════════");
    let (large_inline_amp, _) = run_benchmark(
        "Large Values (64KB, inline)",
        None, // No vlog
        64 * 1024,
        10_000, // Fewer ops for large values
    );

    // Test 5: Large values (64KB) - vlog mode
    println!("\n\n═══════════════════════════════════════");
    println!("Test 5: Large Values (64KB, vlog)");
    println!("═══════════════════════════════════════");
    let (large_vlog_amp, _) = run_benchmark(
        "Large Values (64KB, vlog)",
        Some(4096), // 4KB threshold
        64 * 1024,
        10_000,
    );

    // Summary
    println!("\n\n╔═══════════════════════════════════════════════════════╗");
    println!("║   Summary: Write Amplification Results               ║");
    println!("╚═══════════════════════════════════════════════════════╝\n");

    println!("Small values (1KB):");
    println!("  Inline:     {:.2}x write amp", small_inline_amp);
    println!();

    println!("Medium values (8KB):");
    println!("  Inline:     {:.2}x write amp", medium_inline_amp);
    println!("  vLog:       {:.2}x write amp", medium_vlog_amp);
    let medium_reduction = medium_inline_amp / medium_vlog_amp;
    println!("  Reduction:  {:.2}x better with vlog", medium_reduction);
    println!();

    println!("Large values (64KB):");
    println!("  Inline:     {:.2}x write amp", large_inline_amp);
    println!("  vLog:       {:.2}x write amp", large_vlog_amp);
    let large_reduction = large_inline_amp / large_vlog_amp;
    println!("  Reduction:  {:.2}x better with vlog", large_reduction);
    println!();

    println!("╔═══════════════════════════════════════════════════════╗");
    println!("║   Comparison to Literature                            ║");
    println!("╚═══════════════════════════════════════════════════════╝\n");

    println!("WiscKey paper claims:");
    println!("  - Traditional LSM: 10-30x write amplification");
    println!("  - WiscKey (vlog):  <5x write amplification");
    println!("  - Reduction:       5-10x improvement");
    println!();

    println!("seerdb results:");
    println!("  - Inline (8KB):    {:.2}x write amp", medium_inline_amp);
    println!("  - vLog (8KB):      {:.2}x write amp", medium_vlog_amp);
    println!("  - Reduction:       {:.2}x improvement", medium_reduction);
    println!();
    println!("  - Inline (64KB):   {:.2}x write amp", large_inline_amp);
    println!("  - vLog (64KB):     {:.2}x write amp", large_vlog_amp);
    println!("  - Reduction:       {:.2}x improvement", large_reduction);
    println!();

    if large_reduction >= 5.0 {
        println!("✅ vlog achieves 5-10x write amp reduction for large values!");
        println!("   SOTA claim VALIDATED");
    } else if large_reduction >= 2.0 {
        println!("⚠️  vlog achieves {:.1}x reduction (better than baseline, but below 5-10x claim)", large_reduction);
        println!("   Partial validation");
    } else {
        println!("❌ vlog only achieves {:.1}x reduction (below expectations)", large_reduction);
        println!("   Need investigation");
    }
}

// Check how many SSTables are created and their structure

use seerdb::{DBOptions, DB};
use std::fs;
use tempfile::tempdir;

const NUM_KEYS: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn main() {
    println!("=== SSTable Structure Analysis ===\n");

    let dir = tempdir().unwrap();
    let data_dir = dir.path().to_path_buf();

    let mut opts = DBOptions::default();
    opts.data_dir = data_dir.clone();
    opts.memtable_capacity = 256 * 1024 * 1024;
    opts.wal_sync_policy = seerdb::SyncPolicy::None;
    opts.background_compaction = false;

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    println!("Writing {} keys...", NUM_KEYS);
    for i in 0..NUM_KEYS {
        let key = format!("key{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }

    println!("Flushing...\n");
    db.flush().unwrap();

    // Count SSTables
    let mut sstable_count = 0;
    let mut total_size = 0u64;

    if let Ok(entries) = fs::read_dir(&data_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().map(|s| s == "sst").unwrap_or(false) {
                    sstable_count += 1;
                    if let Ok(metadata) = fs::metadata(&path) {
                        let size_mb = metadata.len() as f64 / 1_000_000.0;
                        total_size += metadata.len();
                        println!("  {}: {:.2} MB", path.file_name().unwrap().to_string_lossy(), size_mb);
                    }
                }
            }
        }
    }

    println!();
    println!("Total SSTables: {}", sstable_count);
    println!("Total SSTable size: {:.2} MB", total_size as f64 / 1_000_000.0);
    println!("Average SSTable size: {:.2} MB", (total_size as f64 / sstable_count as f64) / 1_000_000.0);

    println!("\n=== Analysis ===");
    println!("If there's only 1 SSTable, read overhead is minimal.");
    println!("If there are many SSTables, each read must check multiple files.");
    println!("Expected: 1 SSTable after single flush with no compaction.");
}

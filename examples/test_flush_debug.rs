// Debug flush to see what's happening

use seerdb::{DBOptions, DB};
use tempfile::tempdir;

fn main() {
    let dir = tempdir().unwrap();
    let mut opts = DBOptions::default();
    opts.data_dir = dir.path().to_path_buf();
    opts.memtable_capacity = 256 * 1024 * 1024;
    opts.wal_sync_policy = seerdb::SyncPolicy::None;

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; 1024];

    println!("Writing 1000 keys...");
    for i in 0..1000 {
        let key = format!("key{:04}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }

    let memtable_size = db.memtable_size();
    let memtable_len = db.memtable_len();
    println!("Before flush:");
    println!("  Memtable size: {} bytes", memtable_size);
    println!("  Memtable entries: {}", memtable_len);

    println!("\nFlushing...");
    db.flush().unwrap();

    let memtable_size_after = db.memtable_size();
    let memtable_len_after = db.memtable_len();
    println!("\nAfter flush:");
    println!("  Memtable size: {} bytes", memtable_size_after);
    println!("  Memtable entries: {}", memtable_len_after);

    // Test reads
    println!("\nTesting reads after flush:");
    let mut found = 0;
    let mut not_found = 0;

    for i in 0..1000 {
        let key = format!("key{:04}", i);
        let result = db.get(key.as_bytes()).unwrap();
        if result.is_some() {
            found += 1;
        } else {
            not_found += 1;
            if not_found <= 10 {
                println!("  Key '{}' NOT FOUND", key);
            }
        }
    }

    println!("\nResults:");
    println!("  Found: {}", found);
    println!("  Not found: {}", not_found);
    println!("  Success rate: {:.1}%", (found as f64 / 1000.0) * 100.0);
}

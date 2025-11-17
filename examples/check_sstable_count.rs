// Check how many SSTables exist during baseline_benchmark sequence
use seerdb::{DBOptions, DB};
use std::fs;
use std::path::PathBuf;

const NUM_OPERATIONS: usize = 100_000;
const VALUE_SIZE: usize = 1024;

fn count_sstables(path: &PathBuf) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries {
            if let Ok(entry) = entry {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("sst") {
                    count += 1;
                }
            }
        }
    }
    count
}

fn main() {
    let path = PathBuf::from("/tmp/check_sst_count");
    let _ = std::fs::remove_dir_all(&path);

    let opts = DBOptions {
        data_dir: path.clone(),
        memtable_capacity: 64 * 1024 * 1024,
        wal_sync_policy: seerdb::SyncPolicy::None,
        background_compaction: true,
        vlog_threshold: Some(4096),
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    println!("Initial SSTables: {}", count_sstables(&path));

    // Workload 1: Write 100K
    for i in 0..NUM_OPERATIONS {
        let key = format!("key_{:08}", i);
        db.put(key.as_bytes(), &value).unwrap();
    }
    std::thread::sleep(std::time::Duration::from_millis(500)); // Let compaction settle
    println!("After 100K writes: {} SSTables", count_sstables(&path));

    // Workload 2: Read 100K
    for i in 0..NUM_OPERATIONS {
        let key = format!("key_{:08}", i);
        let _ = db.get(key.as_bytes()).unwrap();
    }
    println!("After 100K reads: {} SSTables", count_sstables(&path));

    // Workload 3: Mixed (50K more writes)
    for i in 0..NUM_OPERATIONS {
        if i % 2 == 0 {
            let key = format!("key_{:08}", i + NUM_OPERATIONS);
            db.put(key.as_bytes(), &value).unwrap();
        } else {
            let key = format!("key_{:08}", i);
            let _ = db.get(key.as_bytes()).unwrap();
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(500)); // Let compaction settle
    println!("After mixed workload: {} SSTables", count_sstables(&path));

    // Now range scan
    use std::time::Instant;
    let start = Instant::now();
    for i in 0..1000 {
        let start_key = format!("key_{:08}", i * 100);
        let end_key = format!("key_{:08}", i * 100 + 100);
        let mut count = 0;
        for result in db
            .range(start_key.as_bytes(), Some(end_key.as_bytes()))
            .unwrap()
        {
            let _ = result.unwrap();
            count += 1;
            if count >= 100 {
                break;
            }
        }
    }
    let elapsed = start.elapsed();
    println!(
        "\nRange scans: {:.0} scans/sec",
        1000.0 / elapsed.as_secs_f64()
    );
    println!("Final SSTable count: {}", count_sstables(&path));
}

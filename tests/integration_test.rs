// Integration tests for WAL + Memtable + SSTable
// Tests the complete write → flush → recovery flow

use bytes::Bytes;
use seerdb::wal::{reader::WALReader, SyncPolicy};
use seerdb::{Memtable, Record, SSTable, WAL};
use tempfile::tempdir;

#[test]
fn test_wal_memtable_integration() {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("test.wal");

    // Create WAL and memtable
    let mut wal = WAL::create(&wal_path, SyncPolicy::SyncAll).unwrap();
    let memtable = Memtable::new(1024 * 1024);

    // Write some data
    let records = vec![
        Record::Put {
            key: Bytes::from("key1"),
            value: Bytes::from("value1"),
        },
        Record::Put {
            key: Bytes::from("key2"),
            value: Bytes::from("value2"),
        },
        Record::Delete {
            key: Bytes::from("key3"),
        },
    ];

    // Write to WAL and memtable
    for record in &records {
        wal.write(record).unwrap();

        match record {
            Record::Put { key, value } => {
                memtable.put(key.clone(), value.clone());
            }
            Record::Delete { key } => {
                memtable.delete(key.clone());
            }
            Record::Merge { .. } => {}
            Record::Batch { operations } => {
                for op in operations {
                    match op {
                        seerdb::wal::BatchOp::Put { key, value } => {
                            memtable.put(key.clone(), value.clone());
                        }
                        seerdb::wal::BatchOp::Delete { key } => {
                            memtable.delete(key.clone());
                        }
                        seerdb::wal::BatchOp::Merge { .. } => {}
                    }
                }
            }
        }
    }

    // Verify memtable has the data
    assert_eq!(memtable.get(b"key1"), Some(Bytes::from("value1")));
    assert_eq!(memtable.get(b"key2"), Some(Bytes::from("value2")));
    assert_eq!(memtable.get(b"key3"), None); // Deleted

    // Flush memtable to SSTable
    let sstable_path = dir.path().join("flush.sst");
    memtable.flush(&sstable_path).unwrap();

    // Open the SSTable and verify it has the data (non-tombstone entries)
    let mut sstable = SSTable::open(&sstable_path).unwrap();
    assert_eq!(sstable.get(b"key1").unwrap(), Some(Bytes::from("value1")));
    assert_eq!(sstable.get(b"key2").unwrap(), Some(Bytes::from("value2")));
    assert_eq!(sstable.get(b"key3").unwrap(), None); // Tombstone not flushed
}

#[test]
fn test_crash_recovery() {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("test.wal");
    let sstable_path = dir.path().join("flush.sst");

    // Simulate writes before crash
    {
        let mut wal = WAL::create(&wal_path, SyncPolicy::SyncAll).unwrap();
        let memtable = Memtable::new(1024 * 1024);

        // Write data
        for i in 0..100 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            let record = Record::Put {
                key: Bytes::from(key.clone()),
                value: Bytes::from(value.clone()),
            };

            wal.write(&record).unwrap();
            memtable.put(Bytes::from(key), Bytes::from(value));
        }

        // Flush some data
        memtable.flush(&sstable_path).unwrap();

        // Simulate crash (drop WAL and memtable)
    }

    // Recovery: Replay WAL
    let mut reader = WALReader::open(&wal_path).unwrap();
    let records = reader.read_all().unwrap();

    // Should have 100 records
    assert_eq!(records.len(), 100);

    // Rebuild memtable from WAL
    let memtable = Memtable::new(1024 * 1024);
    for record in records {
        match record {
            Record::Put { key, value } => {
                memtable.put(key, value);
            }
            Record::Delete { key } => {
                memtable.delete(key);
            }
            Record::Merge { .. } => {}
            Record::Batch { operations } => {
                for op in operations {
                    match op {
                        seerdb::wal::BatchOp::Put { key, value } => {
                            memtable.put(key, value);
                        }
                        seerdb::wal::BatchOp::Delete { key } => {
                            memtable.delete(key);
                        }
                        seerdb::wal::BatchOp::Merge { .. } => {}
                    }
                }
            }
        }
    }

    // Verify data is recovered
    for i in 0..100 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        assert_eq!(memtable.get(key.as_bytes()), Some(Bytes::from(value)));
    }

    // Verify SSTable still has data
    let mut sstable = SSTable::open(&sstable_path).unwrap();
    assert_eq!(sstable.get(b"key_0").unwrap(), Some(Bytes::from("value_0")));
}

#[test]
fn test_write_flush_recover_cycle() {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("test.wal");

    // Phase 1: Write data
    {
        let mut wal = WAL::create(&wal_path, SyncPolicy::SyncData).unwrap();
        let memtable = Memtable::new(100); // Small capacity to trigger flush

        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = format!("value_with_long_data_{}", i);
            let record = Record::Put {
                key: Bytes::from(key.clone()),
                value: Bytes::from(value.clone()),
            };

            wal.write(&record).unwrap();
            memtable.put(Bytes::from(key), Bytes::from(value));

            // Check if should flush
            if memtable.should_flush() {
                let sstable_path = dir.path().join(format!("sstable_{}.sst", i));
                memtable.flush(&sstable_path).unwrap();
            }
        }
    }

    // Phase 2: Recover from WAL
    {
        let mut reader = WALReader::open(&wal_path).unwrap();
        let records = reader.read_all().unwrap();

        let memtable = Memtable::new(100);
        for record in records {
            match record {
                Record::Put { key, value } => {
                    memtable.put(key, value);
                }
                Record::Delete { key } => {
                    memtable.delete(key);
                }
                Record::Merge { .. } => {}
                Record::Batch { operations } => {
                    for op in operations {
                        match op {
                            seerdb::wal::BatchOp::Put { key, value } => {
                                memtable.put(key, value);
                            }
                            seerdb::wal::BatchOp::Delete { key } => {
                                memtable.delete(key);
                            }
                            seerdb::wal::BatchOp::Merge { .. } => {}
                        }
                    }
                }
            }
        }

        // Verify all data recovered
        for i in 0..10 {
            let key = format!("key_{}", i);
            assert!(memtable.get(key.as_bytes()).is_some());
        }
    }
}

#[test]
fn test_delete_in_wal_and_memtable() {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("test.wal");

    // Write, then delete
    {
        let mut wal = WAL::create(&wal_path, SyncPolicy::SyncAll).unwrap();
        let memtable = Memtable::new(1024);

        // Put
        wal.write(&Record::Put {
            key: Bytes::from("key1"),
            value: Bytes::from("value1"),
        })
        .unwrap();
        memtable.put(Bytes::from("key1"), Bytes::from("value1"));

        // Delete
        wal.write(&Record::Delete {
            key: Bytes::from("key1"),
        })
        .unwrap();
        memtable.delete(Bytes::from("key1"));

        assert_eq!(memtable.get(b"key1"), None);
    }

    // Recover
    {
        let mut reader = WALReader::open(&wal_path).unwrap();
        let records = reader.read_all().unwrap();

        let memtable = Memtable::new(1024);
        for record in records {
            match record {
                Record::Put { key, value } => {
                    memtable.put(key, value);
                }
                Record::Delete { key } => {
                    memtable.delete(key);
                }
                Record::Merge { .. } => {}
                Record::Batch { operations } => {
                    for op in operations {
                        match op {
                            seerdb::wal::BatchOp::Put { key, value } => {
                                memtable.put(key, value);
                            }
                            seerdb::wal::BatchOp::Delete { key } => {
                                memtable.delete(key);
                            }
                            seerdb::wal::BatchOp::Merge { .. } => {}
                        }
                    }
                }
            }
        }

        // After recovery, key1 should still be deleted
        assert_eq!(memtable.get(b"key1"), None);
    }
}

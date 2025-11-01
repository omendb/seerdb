// Main database interface
// Integrates WAL, Memtable, SSTable, and Compaction

use crate::compaction::{compact_sstables, LSMTree};
use crate::memtable::{Entry, Memtable};
use crate::sstable::SSTable;
use crate::wal::{Record, SyncPolicy, WAL};
use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DBError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("WAL error: {0}")]
    Wal(#[from] crate::wal::WALError),

    #[error("SSTable error: {0}")]
    SSTable(#[from] crate::sstable::SSTableError),

    #[error("Compaction error: {0}")]
    Compaction(#[from] crate::compaction::CompactionError),

    #[error("Database not opened")]
    NotOpened,
}

pub type Result<T> = std::result::Result<T, DBError>;

/// Main database configuration
#[derive(Debug, Clone)]
pub struct DBOptions {
    /// Directory for database files
    pub data_dir: PathBuf,
    /// Memtable capacity in bytes (default: 64MB)
    pub memtable_capacity: usize,
    /// WAL sync policy (default: SyncData)
    pub wal_sync_policy: SyncPolicy,
    /// LSM base level size (default: 10MB)
    pub base_level_size: u64,
    /// LSM size ratio between levels (default: 10)
    pub size_ratio: u64,
    /// Number of LSM levels (default: 7)
    pub num_levels: usize,
}

impl Default for DBOptions {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./seerdb_data"),
            memtable_capacity: 64 * 1024 * 1024, // 64MB
            wal_sync_policy: SyncPolicy::SyncData,
            base_level_size: 10 * 1024 * 1024, // 10MB
            size_ratio: 10,
            num_levels: 7,
        }
    }
}

/// Main database structure
pub struct DB {
    /// Database options
    options: DBOptions,
    /// Write-ahead log
    wal: Arc<Mutex<WAL>>,
    /// Active memtable
    memtable: Arc<Memtable>,
    /// LSM tree for level management
    lsm: Arc<Mutex<LSMTree>>,
    /// Counter for generating SSTable filenames
    sstable_counter: Arc<Mutex<u64>>,
}

impl DB {
    /// Open or create a database
    pub fn open(options: DBOptions) -> Result<Self> {
        // Create data directory if it doesn't exist
        std::fs::create_dir_all(&options.data_dir)?;

        // Open or create WAL
        let wal_path = options.data_dir.join("wal.log");
        let wal = WAL::create(&wal_path, options.wal_sync_policy)?;

        // Create memtable
        let memtable = Memtable::new(options.memtable_capacity);

        // Create LSM tree
        let lsm = LSMTree::new(
            &options.data_dir,
            options.base_level_size,
            options.size_ratio,
            options.num_levels,
        );

        Ok(Self {
            options: options.clone(),
            wal: Arc::new(Mutex::new(wal)),
            memtable: Arc::new(memtable),
            lsm: Arc::new(Mutex::new(lsm)),
            sstable_counter: Arc::new(Mutex::new(0)),
        })
    }

    /// Put a key-value pair
    pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()> {
        let key = Bytes::copy_from_slice(key.as_ref());
        let value = Bytes::copy_from_slice(value.as_ref());

        // Write to WAL first (durability)
        let record = Record::Put {
            key: key.clone(),
            value: value.clone(),
        };
        self.wal.lock().unwrap().write(&record)?;

        // Write to memtable
        self.memtable.put(key, value);

        // Check if memtable should be flushed
        if self.memtable.should_flush() {
            self.flush()?;
        }

        Ok(())
    }

    /// Get a value by key
    pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Bytes>> {
        let key = key.as_ref();

        // Check memtable first (most recent data)
        if let Some(value) = self.memtable.get(key) {
            return Ok(Some(value));
        }

        // Check SSTables in LSM tree (L0 -> L6)
        let lsm = self.lsm.lock().unwrap();
        for level_num in 0..lsm.num_levels() {
            if let Some(level) = lsm.level(level_num) {
                // Check each SSTable in this level
                for sstable_path in level.sstables() {
                    let mut sstable = SSTable::open(sstable_path)?;
                    if let Some(value) = sstable.get(key)? {
                        return Ok(Some(value));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Delete a key
    pub fn delete(&self, key: impl AsRef<[u8]>) -> Result<()> {
        let key = Bytes::copy_from_slice(key.as_ref());

        // Write to WAL (durability)
        let record = Record::Delete { key: key.clone() };
        self.wal.lock().unwrap().write(&record)?;

        // Write tombstone to memtable
        self.memtable.delete(key);

        // Check if memtable should be flushed
        if self.memtable.should_flush() {
            self.flush()?;
        }

        Ok(())
    }

    /// Flush memtable to L0 SSTable
    fn flush(&self) -> Result<()> {
        // Generate SSTable filename
        let mut counter = self.sstable_counter.lock().unwrap();
        let sstable_path = self
            .options
            .data_dir
            .join(format!("L0_{:06}.sst", *counter));
        *counter += 1;
        drop(counter);

        // Flush memtable to SSTable
        let sstable = self.memtable.flush(&sstable_path)?;
        let size = std::fs::metadata(&sstable_path)?.len();

        // Add to LSM tree L0
        let mut lsm = self.lsm.lock().unwrap();
        lsm.add_l0_sstable(sstable_path, size);

        // Check if compaction is needed
        if let Some(level_num) = lsm.needs_compaction() {
            drop(lsm); // Release lock before compaction
            self.compact_level(level_num)?;
        }

        Ok(())
    }

    /// Compact a level
    fn compact_level(&self, level_num: usize) -> Result<()> {
        let mut lsm = self.lsm.lock().unwrap();

        // Get SSTables to compact
        let level = lsm.level(level_num).ok_or(DBError::NotOpened)?;
        let input_paths: Vec<PathBuf> = level.sstables().to_vec();

        if input_paths.is_empty() {
            return Ok(());
        }

        // Generate output path
        let mut counter = self.sstable_counter.lock().unwrap();
        let output_path = self.options.data_dir.join(format!(
            "L{}_{:06}.sst",
            level_num + 1,
            *counter
        ));
        *counter += 1;
        drop(counter);

        drop(lsm); // Release lock during compaction

        // Compact SSTables
        let (result_path, size) = compact_sstables(&input_paths, &output_path)?;

        // Update LSM tree
        // TODO: This is simplified - need proper level management
        // For now, just add to next level
        let mut lsm = self.lsm.lock().unwrap();
        // lsm.add_to_level(level_num + 1, result_path, size);

        // TODO: Delete input SSTables
        // for path in input_paths {
        //     std::fs::remove_file(path)?;
        // }

        Ok(())
    }

    /// Get current memtable size
    pub fn memtable_size(&self) -> usize {
        self.memtable.size()
    }

    /// Get number of entries in memtable
    pub fn memtable_len(&self) -> usize {
        self.memtable.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_db_open() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();
        assert_eq!(db.memtable_size(), 0);
    }

    #[test]
    fn test_db_put_get() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        db.put(b"key1", b"value1").unwrap();
        db.put(b"key2", b"value2").unwrap();

        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("value1")));
        assert_eq!(db.get(b"key2").unwrap(), Some(Bytes::from("value2")));
        assert_eq!(db.get(b"key3").unwrap(), None);
    }

    #[test]
    fn test_db_delete() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        db.put(b"key1", b"value1").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("value1")));

        db.delete(b"key1").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), None);
    }

    #[test]
    fn test_db_overwrite() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        db.put(b"key1", b"old_value").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("old_value")));

        db.put(b"key1", b"new_value").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("new_value")));
    }

    #[test]
    fn test_db_flush() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 100, // Small capacity to trigger flush
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Write enough data to trigger flush
        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = format!("value_with_long_data_{}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Data should still be accessible after flush
        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = format!("value_with_long_data_{}", i);
            assert_eq!(
                db.get(key.as_bytes()).unwrap(),
                Some(Bytes::from(value))
            );
        }

        // Check that SSTable files were created
        let sst_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s == "sst")
                    .unwrap_or(false)
            })
            .collect();

        assert!(sst_files.len() > 0, "No SSTable files created");
    }
}

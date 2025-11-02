// Main database interface
// Integrates WAL, Memtable, SSTable, Compaction, and VLog

use crate::compaction::{compact_sstables, LSMTree};
use crate::memtable::Memtable;
use crate::sstable::SSTable;
use crate::vlog::VLog;
use crate::wal::{Record, SyncPolicy, WAL, WALReader};
use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
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

    #[error("VLog error: {0}")]
    VLog(#[from] crate::vlog::VLogError),

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
    /// VLog threshold: values larger than this go to vLog (default: None = disabled)
    /// Set to Some(4096) for 4KB threshold (good for embeddings)
    pub vlog_threshold: Option<usize>,
    /// Enable background compaction (default: false for compatibility)
    /// When true, compaction runs in background thread (non-blocking writes)
    pub background_compaction: bool,
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
            vlog_threshold: None,          // Disabled by default
            background_compaction: false,  // Disabled by default for compatibility
        }
    }
}

/// Compaction task message
enum CompactionTask {
    /// Compact a specific level
    CompactLevel(usize),
    /// Shutdown signal
    Shutdown,
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
    /// Value log for KV separation (optional)
    vlog: Arc<Mutex<Option<VLog>>>,
    /// Counter for generating SSTable filenames
    sstable_counter: Arc<Mutex<u64>>,
    /// Channel for sending compaction tasks to background thread
    compaction_tx: Option<Sender<CompactionTask>>,
    /// Background compaction worker thread
    compaction_worker: Option<JoinHandle<()>>,
}

impl DB {
    /// Open or create a database
    pub fn open(options: DBOptions) -> Result<Self> {
        // Create data directory if it doesn't exist
        std::fs::create_dir_all(&options.data_dir)?;

        let wal_path = options.data_dir.join("wal.log");
        let vlog_path = options.data_dir.join("values.vlog");

        // Create memtable
        let memtable = Memtable::new(options.memtable_capacity);

        // Recover from WAL if it exists
        if wal_path.exists() {
            Self::recover(&wal_path, &memtable)?;
        }

        // Create new WAL (overwrites old one after recovery)
        let wal = WAL::create(&wal_path, options.wal_sync_policy)?;

        // Create or open vLog if KV separation is enabled
        let vlog = if options.vlog_threshold.is_some() {
            if vlog_path.exists() {
                Some(VLog::open(&vlog_path)?)
            } else {
                Some(VLog::create(&vlog_path)?)
            }
        } else {
            None
        };

        // Create LSM tree
        let lsm = LSMTree::new(
            &options.data_dir,
            options.base_level_size,
            options.size_ratio,
            options.num_levels,
        );

        let lsm = Arc::new(Mutex::new(lsm));
        let sstable_counter = Arc::new(Mutex::new(0));

        // Start background compaction worker if enabled
        let (compaction_tx, compaction_worker) = if options.background_compaction {
            let (tx, rx) = channel::<CompactionTask>();

            // Clone references for worker thread
            let lsm_clone = Arc::clone(&lsm);
            let sstable_counter_clone = Arc::clone(&sstable_counter);
            let data_dir = options.data_dir.clone();

            // Spawn compaction worker thread
            let worker = thread::spawn(move || {
                while let Ok(task) = rx.recv() {
                    match task {
                        CompactionTask::CompactLevel(level_num) => {
                            // Perform compaction
                            if let Err(e) = Self::run_compaction(
                                &lsm_clone,
                                &sstable_counter_clone,
                                &data_dir,
                                level_num,
                            ) {
                                eprintln!("Background compaction error: {}", e);
                            }
                        }
                        CompactionTask::Shutdown => {
                            // Exit worker thread
                            break;
                        }
                    }
                }
            });

            (Some(tx), Some(worker))
        } else {
            (None, None)
        };

        let db = Self {
            options: options.clone(),
            wal: Arc::new(Mutex::new(wal)),
            memtable: Arc::new(memtable),
            lsm,
            vlog: Arc::new(Mutex::new(vlog)),
            sstable_counter,
            compaction_tx,
            compaction_worker,
        };

        // Flush memtable if it filled up during recovery
        if db.memtable.should_flush() {
            db.flush()?;
        }

        Ok(db)
    }

    /// Recover memtable from WAL
    fn recover(wal_path: &Path, memtable: &Memtable) -> Result<()> {
        let mut reader = WALReader::open(wal_path)
            .map_err(|e| DBError::Io(std::io::Error::other(e)))?;

        let records = reader
            .read_all()
            .map_err(|e| DBError::Io(std::io::Error::other(e)))?;

        for record in records {
            match record {
                Record::Put { key, value } => {
                    memtable.put(key, value);
                }
                Record::Delete { key } => {
                    memtable.delete(key);
                }
            }
        }

        Ok(())
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
        self.wal
            .lock()
            .expect("WAL mutex poisoned")
            .write(&record)?;

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

        // Get vLog if available (need to clone for SSTable attachment)
        let vlog_path = self.options.data_dir.join("values.vlog");
        let has_vlog = self
            .vlog
            .lock()
            .expect("vLog mutex poisoned")
            .is_some();

        // Check SSTables in LSM tree (L0 -> L6)
        let lsm = self.lsm.lock().expect("LSM mutex poisoned");
        for level_num in 0..lsm.num_levels() {
            if let Some(level) = lsm.level(level_num) {
                // Check each SSTable in this level
                for sstable_path in level.sstables() {
                    let mut sstable = if has_vlog {
                        // Attach vLog for reading value pointers
                        let vlog = VLog::open(&vlog_path)?;
                        SSTable::open(sstable_path)?.with_vlog(vlog)
                    } else {
                        SSTable::open(sstable_path)?
                    };

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
        self.wal
            .lock()
            .expect("WAL mutex poisoned")
            .write(&record)?;

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
        use crate::memtable::Entry;
        use crate::sstable::SSTableBuilder;

        // Generate SSTable filename
        let mut counter = self
            .sstable_counter
            .lock()
            .expect("SSTable counter mutex poisoned");
        let sstable_path = self
            .options
            .data_dir
            .join(format!("L0_{:06}.sst", *counter));
        *counter += 1;
        drop(counter);

        // Build SSTable with optional vLog support
        let mut vlog_guard = self.vlog.lock().expect("vLog mutex poisoned");

        let _sstable = if let (Some(threshold), Some(ref mut vlog)) =
            (self.options.vlog_threshold, vlog_guard.as_mut())
        {
            // KV separation enabled - use vLog for large values
            let mut builder = SSTableBuilder::new().with_vlog_threshold(threshold);

            for (key, entry) in self.memtable.iter() {
                match entry {
                    Entry::Value(value) => {
                        builder.add_with_vlog(key, value, vlog)?;
                    }
                    Entry::Tombstone => {
                        // Skip tombstones during flush
                    }
                }
            }

            builder.build(&sstable_path)?
        } else {
            // No KV separation - traditional flush
            drop(vlog_guard); // Release lock
            self.memtable.flush(&sstable_path)?
        };

        let size = std::fs::metadata(&sstable_path)?.len();

        // Add to LSM tree L0
        let mut lsm = self.lsm.lock().expect("LSM mutex poisoned");
        lsm.add_l0_sstable(sstable_path, size);

        // Check if compaction is needed
        if let Some(level_num) = lsm.needs_compaction() {
            drop(lsm); // Release lock before compaction

            if let Some(ref tx) = self.compaction_tx {
                // Background compaction: send signal (non-blocking)
                let _ = tx.send(CompactionTask::CompactLevel(level_num));
            } else {
                // Synchronous compaction: block until done
                self.compact_level(level_num)?;
            }
        }

        Ok(())
    }

    /// Compact a level
    fn compact_level(&self, level_num: usize) -> Result<()> {
        Self::do_compact_level(
            &self.lsm,
            &self.sstable_counter,
            &self.options.data_dir,
            level_num,
        )
    }

    /// Internal compaction implementation (shared by both sync and async paths)
    fn do_compact_level(
        lsm: &Arc<Mutex<LSMTree>>,
        sstable_counter: &Arc<Mutex<u64>>,
        data_dir: &Path,
        level_num: usize,
    ) -> Result<()> {
        let lsm_lock = lsm.lock().expect("LSM mutex poisoned");

        // Get SSTables to compact
        let level = lsm_lock.level(level_num).ok_or(DBError::NotOpened)?;
        let input_paths: Vec<PathBuf> = level.sstables().to_vec();

        if input_paths.is_empty() {
            return Ok(());
        }

        // Generate output path
        let mut counter = sstable_counter
            .lock()
            .expect("SSTable counter mutex poisoned");
        let output_path = data_dir.join(format!(
            "L{}_{:06}.sst",
            level_num + 1,
            *counter
        ));
        *counter += 1;
        drop(counter);

        drop(lsm_lock); // Release lock during compaction

        // Compact SSTables
        let (result_path, size) = compact_sstables(&input_paths, &output_path)?;

        // Update LSM tree - add to next level and remove from current level
        let mut lsm = lsm.lock().expect("LSM mutex poisoned");
        lsm.add_to_level(level_num + 1, result_path, size);
        lsm.remove_sstables_from_level(level_num, &input_paths);
        drop(lsm);

        // Delete input SSTables from disk
        for path in input_paths {
            if let Err(e) = std::fs::remove_file(&path) {
                eprintln!("Warning: Failed to delete SSTable {:?}: {}", path, e);
            }
        }

        Ok(())
    }

    /// Static compaction method for background worker thread
    /// This is called from the worker thread without &self
    fn run_compaction(
        lsm: &Arc<Mutex<LSMTree>>,
        sstable_counter: &Arc<Mutex<u64>>,
        data_dir: &Path,
        level_num: usize,
    ) -> Result<()> {
        Self::do_compact_level(lsm, sstable_counter, data_dir, level_num)
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

/// Graceful shutdown: signal compaction thread to stop and wait for it
impl Drop for DB {
    fn drop(&mut self) {
        if let Some(ref tx) = self.compaction_tx {
            // Send shutdown signal
            let _ = tx.send(CompactionTask::Shutdown);
        }

        // Wait for worker thread to finish
        if let Some(worker) = self.compaction_worker.take() {
            let _ = worker.join();
        }
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

    #[test]
    fn test_db_recovery_basic() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        // Write some data
        {
            let db = DB::open(options.clone()).unwrap();
            db.put(b"key1", b"value1").unwrap();
            db.put(b"key2", b"value2").unwrap();
            db.put(b"key3", b"value3").unwrap();
            // Drop db (simulates shutdown without flush)
        }

        // Reopen and verify data recovered from WAL
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("value1")));
            assert_eq!(db.get(b"key2").unwrap(), Some(Bytes::from("value2")));
            assert_eq!(db.get(b"key3").unwrap(), Some(Bytes::from("value3")));
        }
    }

    #[test]
    fn test_db_recovery_with_deletes() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        // Write and delete some data
        {
            let db = DB::open(options.clone()).unwrap();
            db.put(b"key1", b"value1").unwrap();
            db.put(b"key2", b"value2").unwrap();
            db.delete(b"key1").unwrap(); // Delete key1
            db.put(b"key3", b"value3").unwrap();
        }

        // Reopen and verify recovery
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(db.get(b"key1").unwrap(), None); // Deleted
            assert_eq!(db.get(b"key2").unwrap(), Some(Bytes::from("value2")));
            assert_eq!(db.get(b"key3").unwrap(), Some(Bytes::from("value3")));
        }
    }

    #[test]
    fn test_db_recovery_with_overwrites() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        // Write with overwrites
        {
            let db = DB::open(options.clone()).unwrap();
            db.put(b"key1", b"old_value").unwrap();
            db.put(b"key1", b"new_value").unwrap(); // Overwrite
        }

        // Reopen and verify newest value recovered
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("new_value")));
        }
    }

    #[test]
    fn test_db_recovery_with_flush() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 100, // Small to trigger flush during recovery
            ..Default::default()
        };

        // Write enough data to trigger flush on recovery
        {
            let db = DB::open(options.clone()).unwrap();
            for i in 0..20 {
                let key = format!("key_{}", i);
                let value = format!("value_with_long_data_{}", i);
                db.put(key.as_bytes(), value.as_bytes()).unwrap();
            }
        }

        // Reopen (recovery should trigger flush due to small memtable)
        {
            let db = DB::open(options.clone()).unwrap();
            for i in 0..20 {
                let key = format!("key_{}", i);
                let value = format!("value_with_long_data_{}", i);
                assert_eq!(
                    db.get(key.as_bytes()).unwrap(),
                    Some(Bytes::from(value))
                );
            }
        }
    }

    #[test]
    fn test_db_recovery_empty_wal() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        // Create DB (no data written)
        {
            let _db = DB::open(options.clone()).unwrap();
        }

        // Reopen (WAL exists but is empty)
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(db.get(b"key1").unwrap(), None);
        }
    }

    #[test]
    fn test_db_with_kv_separation() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 200, // Small enough to trigger flush
            vlog_threshold: Some(50), // 50 byte threshold
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Small value (stored inline in SSTable after flush)
        db.put(b"small_key", b"tiny_value").unwrap();

        // Large value (will be stored in vLog after flush)
        let large_value = vec![b'X'; 100];
        db.put(b"large_key", &large_value).unwrap();

        // Write more data to trigger flush
        for i in 0..3 {
            let key = format!("k{}", i);
            let value = format!("value_data_{}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Verify all values can be read (from memtable or flushed SSTable)
        assert_eq!(db.get(b"small_key").unwrap(), Some(Bytes::from("tiny_value")));
        assert_eq!(
            db.get(b"large_key").unwrap(),
            Some(Bytes::from(large_value))
        );

        // Verify vLog file was created
        let vlog_path = dir.path().join("values.vlog");
        assert!(vlog_path.exists(), "vLog file should exist with vlog_threshold enabled");
    }

    #[test]
    fn test_db_with_kv_separation_recovery() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            vlog_threshold: Some(50), // 50 byte threshold
            ..Default::default()
        };

        // Write data with large values
        {
            let db = DB::open(options.clone()).unwrap();
            db.put(b"key1", b"small_value").unwrap();
            let large_value = vec![b'Y'; 200];
            db.put(b"key2", &large_value).unwrap();
        }

        // Reopen and verify recovery works with vLog
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(
                db.get(b"key1").unwrap(),
                Some(Bytes::from("small_value"))
            );
            let expected_large = vec![b'Y'; 200];
            assert_eq!(db.get(b"key2").unwrap(), Some(Bytes::from(expected_large)));
        }
    }

    #[test]
    fn test_db_background_compaction() {
        use std::time::Duration;

        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 100, // Small to trigger flushes
            background_compaction: true, // Enable background compaction
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Write enough data to trigger multiple flushes and compaction
        for i in 0..100 {
            let key = format!("key_{:03}", i);
            let value = format!("value_{:03}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Give background thread time to process compactions
        std::thread::sleep(Duration::from_millis(100));

        // Verify data is still readable
        for i in 0..100 {
            let key = format!("key_{:03}", i);
            let expected = format!("value_{:03}", i);
            assert_eq!(
                db.get(key.as_bytes()).unwrap(),
                Some(Bytes::from(expected))
            );
        }

        // DB will be dropped here, triggering graceful shutdown
    }

    #[test]
    fn test_db_sync_vs_async_compaction() {
        use std::time::Duration;

        // Test that both modes produce identical results
        let dir_sync = tempdir().unwrap();
        let dir_async = tempdir().unwrap();

        let options_sync = DBOptions {
            data_dir: dir_sync.path().to_path_buf(),
            memtable_capacity: 100,
            background_compaction: false, // Synchronous
            ..Default::default()
        };

        let options_async = DBOptions {
            data_dir: dir_async.path().to_path_buf(),
            memtable_capacity: 100,
            background_compaction: true, // Asynchronous
            ..Default::default()
        };

        let db_sync = DB::open(options_sync).unwrap();
        let db_async = DB::open(options_async).unwrap();

        // Write same data to both
        for i in 0..50 {
            let key = format!("key_{:03}", i);
            let value = format!("value_{:03}", i);
            db_sync.put(key.as_bytes(), value.as_bytes()).unwrap();
            db_async.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Give async compaction time to finish
        std::thread::sleep(Duration::from_millis(100));

        // Verify both return same results
        for i in 0..50 {
            let key = format!("key_{:03}", i);
            let expected = format!("value_{:03}", i);
            assert_eq!(
                db_sync.get(key.as_bytes()).unwrap(),
                Some(Bytes::from(expected.clone()))
            );
            assert_eq!(
                db_async.get(key.as_bytes()).unwrap(),
                Some(Bytes::from(expected))
            );
        }
    }
}

//! Point-in-time consistent snapshots for seerdb
//!
//! Snapshots provide consistent read views of the database at a specific point in time.
//! They are critical for:
//! - Consistent multi-read operations (e.g., range scan during writes)
//! - Backup operations
//! - Long-running analytical queries
//!
//! # Implementation
//!
//! Snapshots capture references to:
//! - Active memtable partitions (pinned via Arc)
//! - Immutable memtables being flushed (pinned via Arc)
//! - LSM tree state (pinned via Arc)
//!
//! The snapshot holds Arc references that prevent memtables from being dropped.
//! SSTables are protected by tracking snapshot sequence numbers - compaction
//! won't delete SSTables that snapshots still reference.

use crate::memtable::Memtable;
use crate::range::RangeIterator;
use crate::sstable::SSTable;
use crate::vlog::VLog;
use bytes::Bytes;
use quick_cache::sync::Cache;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A point-in-time consistent snapshot of the database
///
/// Snapshots provide isolation for reads - writes to the database after
/// the snapshot was created are not visible to the snapshot.
///
/// # Thread Safety
///
/// Snapshots are fully thread-safe and can be shared across threads.
/// All internal references use Arc for safe sharing.
///
/// # Memory Management
///
/// Snapshots hold references to memtables and the LSM tree state at
/// creation time. This prevents them from being garbage collected.
/// Long-lived snapshots can increase memory usage.
///
/// # Example
///
/// ```rust,no_run
/// use seerdb::{DB, DBOptions};
///
/// let db = DB::open(DBOptions::default()).unwrap();
/// db.put(b"key1", b"value1").unwrap();
///
/// // Create snapshot
/// let snapshot = db.snapshot();
///
/// // Write after snapshot
/// db.put(b"key1", b"value2").unwrap();
///
/// // Snapshot sees old value
/// assert_eq!(snapshot.get(b"key1").unwrap(), Some(Bytes::from("value1")));
///
/// // DB sees new value
/// assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("value2")));
/// ```
pub struct Snapshot {
    /// Pinned memtable partitions at snapshot time
    memtables: Vec<Arc<Memtable>>,

    /// Pinned immutable memtables (if flush was in progress)
    immutable_memtables: Option<Arc<Vec<Arc<Memtable>>>>,

    /// SSTable paths at snapshot time (indexed by level)
    /// This captures the exact LSM tree state at snapshot creation
    sstable_paths: Vec<Vec<PathBuf>>,

    /// SSTable cache (shared with DB for efficiency)
    sstable_cache: Arc<Cache<PathBuf, Arc<Mutex<SSTable>>>>,

    /// VLog path for value separation (optional)
    vlog_path: Option<PathBuf>,

    /// Whether VLog is enabled
    has_vlog: bool,

    /// Snapshot sequence number for tracking
    sequence_number: u64,
}

impl Snapshot {
    /// Create a new snapshot with the given state
    ///
    /// This is called internally by DB::snapshot()
    pub(crate) fn new(
        memtables: Vec<Arc<Memtable>>,
        immutable_memtables: Option<Arc<Vec<Arc<Memtable>>>>,
        sstable_paths: Vec<Vec<PathBuf>>,
        sstable_cache: Arc<Cache<PathBuf, Arc<Mutex<SSTable>>>>,
        vlog_path: Option<PathBuf>,
        has_vlog: bool,
        sequence_number: u64,
    ) -> Self {
        Self {
            memtables,
            immutable_memtables,
            sstable_paths,
            sstable_cache,
            vlog_path,
            has_vlog,
            sequence_number,
        }
    }

    /// Get a value by key from the snapshot
    ///
    /// Returns the value as it was at snapshot creation time.
    /// Writes after the snapshot was created are not visible.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// let db = DB::open(DBOptions::default()).unwrap();
    /// db.put(b"key", b"old_value").unwrap();
    ///
    /// let snapshot = db.snapshot();
    /// db.put(b"key", b"new_value").unwrap();
    ///
    /// // Snapshot sees old value
    /// assert_eq!(snapshot.get(b"key").unwrap(), Some(Bytes::from("old_value")));
    /// ```
    pub fn get(&self, key: &[u8]) -> crate::db::Result<Option<Bytes>> {
        // Search memtables first (from newest to oldest)
        // Check all active partitions at snapshot time
        for memtable in &self.memtables {
            if let Some(value) = memtable.get(key) {
                // Check for tombstone (deletion marker)
                if value.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(Bytes::from(value.to_vec())));
            }
        }

        // Check immutable memtables (if flush was in progress at snapshot time)
        if let Some(ref immutables) = self.immutable_memtables {
            for memtable in immutables.iter() {
                if let Some(value) = memtable.get(key) {
                    if value.is_empty() {
                        return Ok(None);
                    }
                    return Ok(Some(Bytes::from(value.to_vec())));
                }
            }
        }

        // Search SSTables at snapshot time (from L0 to LN)
        // SSTables handle VLog references internally via with_vlog()
        let vlog_path = self.vlog_path.as_ref().map(|p| p.join("values.vlog"));

        for (level_idx, level_sstables) in self.sstable_paths.iter().enumerate() {
            // L0 has overlapping SSTables - check newest first (reverse order)
            // L1+ have non-overlapping SSTables - check in forward order
            let sstables: Vec<_> = if level_idx == 0 {
                level_sstables.iter().rev().collect()
            } else {
                level_sstables.iter().collect()
            };

            for sstable_path in sstables {
                // Use cache for efficient SSTable access
                let sstable_arc = self.sstable_cache.get_or_insert_with(
                    sstable_path,
                    || -> crate::db::Result<Arc<Mutex<SSTable>>> {
                        // Open SSTable with VLog if enabled
                        let sstable = if self.has_vlog {
                            if let Some(ref vlog_file) = vlog_path {
                                let vlog = VLog::open(vlog_file)?;
                                SSTable::open(sstable_path.clone())?.with_vlog(vlog)
                            } else {
                                SSTable::open(sstable_path.clone())?
                            }
                        } else {
                            SSTable::open(sstable_path.clone())?
                        };
                        Ok(Arc::new(Mutex::new(sstable)))
                    },
                )?;

                let mut sstable_guard = sstable_arc.lock().expect("SSTable lock poisoned");
                if let Ok(Some(value)) = sstable_guard.get(key) {
                    drop(sstable_guard);

                    // Check for tombstone
                    if value.is_empty() {
                        return Ok(None);
                    }

                    return Ok(Some(value));
                }
            }
        }

        // Key not found
        Ok(None)
    }

    /// Iterate over a range of keys in the snapshot
    ///
    /// Returns an iterator over key-value pairs in sorted order.
    /// Only keys that existed at snapshot creation time are included.
    ///
    /// # Arguments
    ///
    /// * `start_key` - Start of range (inclusive)
    /// * `end_key` - End of range (exclusive), None for unbounded
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// let db = DB::open(DBOptions::default()).unwrap();
    /// db.put(b"a", b"1").unwrap();
    /// db.put(b"b", b"2").unwrap();
    /// db.put(b"c", b"3").unwrap();
    ///
    /// let snapshot = db.snapshot();
    ///
    /// // Modify after snapshot
    /// db.put(b"b", b"modified").unwrap();
    /// db.delete(b"c").unwrap();
    ///
    /// // Snapshot sees original values
    /// let results: Vec<_> = snapshot.range(b"a", Some(b"d")).unwrap().collect();
    /// assert_eq!(results.len(), 3); // a, b, c all present
    /// ```
    pub fn range(
        &self,
        start_key: &[u8],
        end_key: Option<&[u8]>,
    ) -> crate::db::Result<RangeIterator> {
        // Collect memtable references
        let mut partition_refs: Vec<&Memtable> =
            self.memtables.iter().map(|arc| arc.as_ref()).collect();

        // Include immutable memtables if present
        if let Some(ref immutables) = self.immutable_memtables {
            partition_refs.extend(immutables.iter().map(|arc| arc.as_ref()));
        }

        // Collect SSTables at snapshot time
        let mut sstables = Vec::new();
        for level_sstables in &self.sstable_paths {
            for sstable_path in level_sstables {
                let sstable_arc = self.sstable_cache.get_or_insert_with(
                    sstable_path,
                    || -> crate::db::Result<Arc<Mutex<SSTable>>> {
                        let sstable = SSTable::open(sstable_path.clone())?;
                        Ok(Arc::new(Mutex::new(sstable)))
                    },
                )?;

                let sstable_guard = sstable_arc.lock().expect("SSTable lock poisoned");
                let overlaps = sstable_guard.overlaps_range(start_key, end_key);

                if overlaps {
                    let iter = sstable_guard.scan_range(start_key, end_key);
                    drop(sstable_guard);
                    sstables.push(iter);
                } else {
                    drop(sstable_guard);
                }
            }
        }

        RangeIterator::new(start_key, end_key, &partition_refs, sstables)
    }

    /// Get the sequence number of this snapshot
    ///
    /// Useful for debugging and tracking snapshot age.
    pub fn sequence_number(&self) -> u64 {
        self.sequence_number
    }
}

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total_sstables: usize = self.sstable_paths.iter().map(|l| l.len()).sum();
        f.debug_struct("Snapshot")
            .field("sequence_number", &self.sequence_number)
            .field("memtable_partitions", &self.memtables.len())
            .field("has_immutable_memtables", &self.immutable_memtables.is_some())
            .field("lsm_levels", &self.sstable_paths.len())
            .field("total_sstables", &total_sstables)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests will be added in db.rs test module
    // since Snapshot requires a full DB instance
}

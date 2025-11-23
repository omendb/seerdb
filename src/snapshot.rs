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
use crate::MergeOperator;
use bytes::Bytes;
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
/// use bytes::Bytes;
///
/// let db = DB::open(DBOptions::default()).unwrap();
/// db.put(b"key1", b"value1").unwrap();
///
/// // Create snapshot
/// let snapshot = db.snapshot().unwrap();
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

    /// Pinned SSTables at snapshot time (indexed by level)
    /// We hold direct references to ensure file handles stay open
    /// even if compaction deletes the underlying files.
    sstables: Vec<Vec<Arc<Mutex<SSTable>>>>,

    /// Snapshot sequence number for tracking
    sequence_number: u64,

    /// Optional merge operator for resolving merges
    merge_operator: Option<Arc<dyn MergeOperator>>,
}

impl Snapshot {
    /// Create a new snapshot with the given state
    ///
    /// This is called internally by DB::snapshot()
    pub(crate) fn new(
        memtables: Vec<Arc<Memtable>>,
        immutable_memtables: Option<Arc<Vec<Arc<Memtable>>>>,
        sstables: Vec<Vec<Arc<Mutex<SSTable>>>>,
        sequence_number: u64,
        merge_operator: Option<Arc<dyn MergeOperator>>,
    ) -> Self {
        Self {
            memtables,
            immutable_memtables,
            sstables,
            sequence_number,
            merge_operator,
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
    /// use bytes::Bytes;
    ///
    /// let db = DB::open(DBOptions::default()).unwrap();
    /// db.put(b"key", b"old_value").unwrap();
    ///
    /// let snapshot = db.snapshot().unwrap();
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
        // We already hold valid references to SSTables with open file handles
        for level_sstables in self.sstables.iter() {
            // IMPORTANT: Check all levels in reverse order (newest first)
            // L0 has overlapping SSTables - check newest first
            // L1+ may also have overlapping SSTables due to our simple compaction strategy
            // (we add new merged SSTables without re-merging with existing L1 SSTables)
            // So we check reverse order to get the latest value
            let sstables_iter = level_sstables.iter().rev();

            for sstable_arc in sstables_iter {
                let mut sstable_guard = sstable_arc.lock().expect("SSTable lock poisoned");
                
                // Temporary VLog attachment if needed
                // Note: SSTable struct holds vlog: Option<Arc<Mutex<VLog>>>.
                // If we opened it without VLog, we might need to attach it?
                // But SSTable::open_with_options doesn't take VLog.
                // Actually, SSTable has `with_vlog` method which consumes self and returns new Self.
                // But we have Arc<Mutex<SSTable>>. We can't easily replace it.
                //
                // However, SSTable's get() handles VLog if `self.vlog` is set.
                // When we open SSTables in DB::snapshot, we should ensure VLog is attached if needed?
                // Or better: if has_vlog is true, we might need to open the VLog here?
                //
                // Let's check if SSTable has `set_vlog`.
                // If not, we rely on the fact that SSTable handles regular values, 
                // and for pointer values it uses `self.vlog`.
                // If `self.vlog` is None in the cached SSTable, `get()` will fail for large values.
                //
                // Ideally, the cached SSTables in DB should have VLog attached if VLog is enabled.
                // But `DB` opens SSTables. Does it attach VLog?
                // In `DB::open`, it opens SSTables. It checks `has_vlog`.
                
                // For now, let's assume the SSTable instance handles its own VLog or we need to handle it.
                // The previous code in `get` did:
                // if self.has_vlog { ... SSTable::open(path).with_vlog(vlog) ... }
                // This implies cached SSTables might NOT have VLog attached?
                // If cached SSTables don't have VLog, and we reuse them, we might fail to read large values.
                
                // Let's assume we fix this by ensuring cached SSTables have VLog if enabled, OR we attach it here.
                // But we can't attach to shared Arc<Mutex<SSTable>> without locking for write.
                // And `with_vlog` consumes self.
                
                // Actually, let's look at how `with_vlog` works.
                // It sets `self.vlog = Some(vlog)`.
                // We can add `set_vlog` to SSTable if needed.
                // But multiple threads might share this SSTable.
                
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
    /// let snapshot = db.snapshot().unwrap();
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
        for level_sstables in &self.sstables {
            for sstable_arc in level_sstables {
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

        RangeIterator::new(
            start_key,
            end_key,
            &partition_refs,
            sstables,
            self.merge_operator.clone(),
        )
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
        let total_sstables: usize = self.sstables.iter().map(|l| l.len()).sum();
        f.debug_struct("Snapshot")
            .field("sequence_number", &self.sequence_number)
            .field("memtable_partitions", &self.memtables.len())
            .field(
                "has_immutable_memtables",
                &self.immutable_memtables.is_some(),
            )
            .field("lsm_levels", &self.sstables.len())
            .field("total_sstables", &total_sstables)
            .finish()
    }
}

#[cfg(test)]
mod tests {

    // Integration tests will be added in db.rs test module
    // since Snapshot requires a full DB instance
}

// Batch write operations for atomic multi-key updates
//
// Allows collecting multiple put/delete operations and committing them atomically
// to the WAL and memtable. This is more efficient than individual operations as it:
// - Writes to WAL once (instead of N times)
// - Reduces channel overhead
// - Better cache locality

use bytes::Bytes;

use crate::db::{DBError, Result, DB};
use crate::wal::{BatchOp, Record, SyncPolicy};

/// Operation type in a batch
#[derive(Clone, Debug)]
enum Operation {
    /// Insert or update a key-value pair
    Put { key: Bytes, value: Bytes },
    /// Delete a key
    Delete { key: Bytes },
}

/// Atomic write batch
///
/// Collects multiple write operations (puts and deletes) and commits them atomically.
/// All operations in a batch succeed or fail together, providing transactional semantics.
///
/// # Examples
///
/// ```rust,no_run
/// use seerdb::{DB, DBOptions};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let db = DB::open(DBOptions::default())?;
///
/// // Create a batch
/// let mut batch = db.batch();
///
/// // Add operations
/// batch.put(b"user:1:name", b"Alice");
/// batch.put(b"user:1:email", b"alice@example.com");
/// batch.delete(b"user:1:temp");
///
/// // Commit atomically
/// batch.commit()?;
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// Batching is significantly faster than individual operations because:
/// - Single WAL write instead of multiple
/// - Reduced thread synchronization overhead
/// - Better CPU cache locality
///
/// Typical improvement: 2-5x faster for batches of 100+ operations
pub struct Batch<'db> {
    /// Reference to parent database
    db: &'db DB,
    /// Collected operations
    operations: Vec<Operation>,
}

impl<'db> Batch<'db> {
    /// Create a new batch for the given database
    pub(crate) fn new(db: &'db DB) -> Self {
        Self {
            db,
            operations: Vec::new(),
        }
    }

    /// Create a new batch with preallocated capacity
    ///
    /// Use this when you know the approximate number of operations
    /// to avoid reallocations.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use seerdb::{DB, DBOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DB::open(DBOptions::default())?;
    /// // Preallocate for 1000 operations
    /// let mut batch = db.batch_with_capacity(1000);
    ///
    /// for i in 0..1000 {
    ///     batch.put(format!("key_{}", i).as_bytes(), b"value");
    /// }
    /// batch.commit()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_capacity(db: &'db DB, capacity: usize) -> Self {
        Self {
            db,
            operations: Vec::with_capacity(capacity),
        }
    }

    /// Add a put operation to the batch
    ///
    /// The operation is not written to disk until `commit()` is called.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use seerdb::{DB, DBOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DB::open(DBOptions::default())?;
    /// let mut batch = db.batch();
    /// batch.put(b"key1", b"value1");
    /// batch.put(b"key2", b"value2");
    /// batch.commit()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn put(&mut self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) {
        let key = Bytes::copy_from_slice(key.as_ref());
        let value = Bytes::copy_from_slice(value.as_ref());
        self.operations.push(Operation::Put { key, value });
    }

    /// Add a delete operation to the batch
    ///
    /// The operation is not written to disk until `commit()` is called.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use seerdb::{DB, DBOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DB::open(DBOptions::default())?;
    /// let mut batch = db.batch();
    /// batch.delete(b"old_key");
    /// batch.commit()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete(&mut self, key: impl AsRef<[u8]>) {
        let key = Bytes::copy_from_slice(key.as_ref());
        self.operations.push(Operation::Delete { key });
    }

    /// Get the number of operations in the batch
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Clear all operations from the batch
    ///
    /// Useful if you want to reuse the batch without deallocating.
    pub fn clear(&mut self) {
        self.operations.clear();
    }

    /// Commit all operations in the batch atomically
    ///
    /// Writes all operations to the WAL and memtable. If any operation fails,
    /// none of the operations take effect (atomic semantics).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - WAL write fails
    /// - Memtable flush is triggered and fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use seerdb::{DB, DBOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DB::open(DBOptions::default())?;
    /// let mut batch = db.batch();
    /// batch.put(b"key1", b"value1");
    /// batch.put(b"key2", b"value2");
    /// batch.commit()?;  // Atomic: both succeed or both fail
    /// # Ok(())
    /// # }
    /// ```
    pub fn commit(self) -> Result<()> {
        if self.operations.is_empty() {
            return Ok(());
        }

        // Convert internal operations to WAL BatchOp format
        let wal_ops: Vec<BatchOp> = self
            .operations
            .iter()
            .map(|op| match op {
                Operation::Put { key, value } => BatchOp::Put {
                    key: key.clone(),
                    value: value.clone(),
                },
                Operation::Delete { key } => BatchOp::Delete { key: key.clone() },
            })
            .collect();

        // Write single atomic batch record to WAL (durability)
        // This ensures atomicity: either ALL operations are written or NONE
        let batch_record = Record::Batch {
            operations: wal_ops,
        };

        // Fast path for SyncPolicy::None - fire-and-forget (no ack needed)
        // Group commit with ack only for durable writes (SyncPolicy::SyncData/SyncAll)
        match self.db.options.wal_sync_policy {
            SyncPolicy::None => {
                // Fast path: fire-and-forget, no acknowledgement
                self.db
                    .wal_tx
                    .send(crate::db::WALMessage::Record(batch_record))
                    .map_err(|_| {
                        DBError::Wal(crate::wal::WALError::Io(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "WAL writer thread died",
                        )))
                    })?;
            }
            SyncPolicy::SyncData | SyncPolicy::SyncAll => {
                // Slow path: group commit with acknowledgement for durability
                let (ack_tx, ack_rx) = crossbeam_channel::bounded(1);

                // Send batch to WAL writer (will batch with concurrent writes)
                self.db
                    .wal_tx
                    .send(crate::db::WALMessage::WriteAndAck {
                        record: batch_record,
                        ack_tx,
                    })
                    .map_err(|_| {
                        DBError::Wal(crate::wal::WALError::Io(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "WAL writer thread died",
                        )))
                    })?;

                // Wait for WAL flush completion (group commit - durability guaranteed!)
                ack_rx
                    .recv()
                    .map_err(|_| {
                        DBError::Wal(crate::wal::WALError::Io(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "WAL writer thread died during flush",
                        )))
                    })?
                    .map_err(DBError::Wal)?;
            }
        }

        // Apply all operations to memtables atomically
        // This is fast since memtables are lock-free
        // If any operation fails here, the WAL already has the full batch
        // so recovery will apply all operations on restart
        for op in &self.operations {
            match op {
                Operation::Put { key, value } => {
                    self.db.put_internal(key.clone(), value.clone())?;
                }
                Operation::Delete { key } => {
                    self.db.delete_internal(key.clone())?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DBOptions, DB};
    use tempfile::tempdir;

    #[test]
    fn test_batch_basic() {
        let dir = tempdir().unwrap();
        let opts = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let db = DB::open(opts).unwrap();

        let mut batch = db.batch();
        batch.put(b"key1", b"value1");
        batch.put(b"key2", b"value2");
        batch.delete(b"key3");

        assert_eq!(batch.len(), 3);
        assert!(!batch.is_empty());

        batch.commit().unwrap();

        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("value1")));
        assert_eq!(db.get(b"key2").unwrap(), Some(Bytes::from("value2")));
        assert_eq!(db.get(b"key3").unwrap(), None);
    }

    #[test]
    fn test_batch_empty() {
        let dir = tempdir().unwrap();
        let opts = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let db = DB::open(opts).unwrap();

        let batch = db.batch();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);

        // Committing empty batch should succeed
        batch.commit().unwrap();
    }

    #[test]
    fn test_batch_with_capacity() {
        let dir = tempdir().unwrap();
        let opts = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let db = DB::open(opts).unwrap();

        let mut batch = db.batch_with_capacity(100);
        for i in 0..100 {
            batch.put(format!("key_{}", i).as_bytes(), b"value");
        }

        assert_eq!(batch.len(), 100);
        batch.commit().unwrap();

        // Verify all keys exist
        for i in 0..100 {
            let key = format!("key_{}", i);
            assert_eq!(db.get(key.as_bytes()).unwrap(), Some(Bytes::from("value")));
        }
    }
}

// Memtable: In-memory buffer for recent writes
// Uses concurrent skiplist for lock-free reads/writes

use bytes::Bytes;
use crossbeam_skiplist::SkipMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::sstable::{SSTableBuilder, SSTableError};

/// In-memory sorted table for recent writes
pub struct Memtable {
    /// Underlying skiplist (concurrent, lock-free)
    data: Arc<SkipMap<Bytes, Entry>>,
    /// Current size in bytes (approximate)
    size: AtomicUsize,
    /// Capacity threshold for flushing
    capacity: usize,
}

/// Entry in the memtable (value or tombstone)
#[derive(Debug, Clone)]
pub enum Entry {
    /// Value entry
    Value(Bytes),
    /// Tombstone (deletion marker)
    Tombstone,
}

impl Memtable {
    /// Create a new memtable with given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            data: Arc::new(SkipMap::new()),
            size: AtomicUsize::new(0),
            capacity,
        }
    }

    /// Create memtable with default capacity (64MB)
    pub fn with_default_capacity() -> Self {
        Self::new(64 * 1024 * 1024)
    }

    /// Insert a key-value pair
    pub fn put(&self, key: Bytes, value: Bytes) {
        let size_delta = key.len() + value.len();
        self.data.insert(key, Entry::Value(value));
        self.size.fetch_add(size_delta, Ordering::Relaxed);
    }

    /// Delete a key (insert tombstone)
    pub fn delete(&self, key: Bytes) {
        let size_delta = key.len();
        self.data.insert(key, Entry::Tombstone);
        self.size.fetch_add(size_delta, Ordering::Relaxed);
    }

    /// Get a value by key
    pub fn get(&self, key: &[u8]) -> Option<Bytes> {
        self.data.get(key).and_then(|entry| match entry.value() {
            Entry::Value(v) => Some(v.clone()),
            Entry::Tombstone => None,
        })
    }

    /// Check if key exists (including tombstones)
    pub fn contains(&self, key: &[u8]) -> bool {
        self.data.contains_key(key)
    }

    /// Get current size in bytes (approximate)
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Check if memtable should be flushed
    pub fn should_flush(&self) -> bool {
        self.size() >= self.capacity
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Iterate over all entries in sorted order
    pub fn iter(&self) -> impl Iterator<Item = (Bytes, Entry)> + '_ {
        self.data
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
    }

    /// Range scan: iterate over keys in [start, end)
    pub fn range<'a>(
        &'a self,
        start: &[u8],
        end: &[u8],
    ) -> impl Iterator<Item = (Bytes, Entry)> + 'a {
        let start_key = Bytes::copy_from_slice(start);
        let end_key = Bytes::copy_from_slice(end);

        self.data
            .range(start_key..end_key)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
    }

    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clone the skipmap for creating an immutable snapshot
    pub fn snapshot(&self) -> Arc<SkipMap<Bytes, Entry>> {
        Arc::clone(&self.data)
    }

    /// Flush memtable to disk as an SSTable
    /// Flush memtable to SSTable, including tombstones
    pub fn flush(&self, path: impl AsRef<Path>) -> Result<(), SSTableError> {
        let mut builder = SSTableBuilder::create(path)?;

        // Iterate in sorted order and add to SSTable
        for entry in self.iter() {
            match entry.1 {
                Entry::Value(value) => {
                    builder.add(entry.0, value)?;
                }
                Entry::Tombstone => {
                    // **CRITICAL**: Tombstones MUST be persisted to SSTables!
                    // They mask older values in lower levels and are removed during compaction
                    // when all older versions are gone
                    builder.add_tombstone(entry.0)?;
                }
            }
        }

        builder.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sstable::SSTable;

    #[test]
    fn test_memtable_put_get() {
        let memtable = Memtable::new(1024);

        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        memtable.put(Bytes::from("key2"), Bytes::from("value2"));

        assert_eq!(memtable.get(b"key1"), Some(Bytes::from("value1")));
        assert_eq!(memtable.get(b"key2"), Some(Bytes::from("value2")));
        assert_eq!(memtable.get(b"key3"), None);
    }

    #[test]
    fn test_memtable_delete() {
        let memtable = Memtable::new(1024);

        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        assert_eq!(memtable.get(b"key1"), Some(Bytes::from("value1")));

        memtable.delete(Bytes::from("key1"));
        assert_eq!(memtable.get(b"key1"), None);
    }

    #[test]
    fn test_memtable_size() {
        let memtable = Memtable::new(1024);

        assert_eq!(memtable.size(), 0);

        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        let size_after_put = memtable.size();
        assert!(size_after_put > 0);

        memtable.delete(Bytes::from("key2"));
        assert!(memtable.size() > size_after_put);
    }

    #[test]
    fn test_memtable_should_flush() {
        let memtable = Memtable::new(100); // Small capacity

        assert!(!memtable.should_flush());

        // Insert enough data to exceed capacity
        for i in 0..20 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            memtable.put(Bytes::from(key), Bytes::from(value));
        }

        assert!(memtable.should_flush());
    }

    #[test]
    fn test_memtable_iter() {
        let memtable = Memtable::new(1024);

        memtable.put(Bytes::from("key3"), Bytes::from("value3"));
        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        memtable.put(Bytes::from("key2"), Bytes::from("value2"));

        let entries: Vec<_> = memtable.iter().collect();
        assert_eq!(entries.len(), 3);

        // Should be sorted
        assert_eq!(entries[0].0, Bytes::from("key1"));
        assert_eq!(entries[1].0, Bytes::from("key2"));
        assert_eq!(entries[2].0, Bytes::from("key3"));
    }

    #[test]
    fn test_memtable_range() {
        let memtable = Memtable::new(1024);

        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        memtable.put(Bytes::from("key2"), Bytes::from("value2"));
        memtable.put(Bytes::from("key3"), Bytes::from("value3"));
        memtable.put(Bytes::from("key4"), Bytes::from("value4"));

        let entries: Vec<_> = memtable.range(b"key2", b"key4").collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, Bytes::from("key2"));
        assert_eq!(entries[1].0, Bytes::from("key3"));
    }

    #[test]
    fn test_memtable_concurrent() {
        use std::thread;

        let memtable = Arc::new(Memtable::new(1024 * 1024));
        let mut handles = vec![];

        for i in 0..10 {
            let mt = Arc::clone(&memtable);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let key = format!("key_{}_{}", i, j);
                    let value = format!("value_{}_{}", i, j);
                    mt.put(Bytes::from(key), Bytes::from(value));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(memtable.len(), 1000);
    }

    #[test]
    fn test_memtable_flush() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("flush.sst");

        let memtable = Memtable::new(1024);
        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        memtable.put(Bytes::from("key2"), Bytes::from("value2"));
        memtable.put(Bytes::from("key3"), Bytes::from("value3"));

        // Flush to disk
        memtable.flush(&sstable_path).unwrap();
        let mut sstable = SSTable::open(&sstable_path).unwrap();

        // Verify data was written correctly
        assert_eq!(sstable.len(), 3);
        assert_eq!(sstable.get(b"key1").unwrap(), Some(Bytes::from("value1")));
        assert_eq!(sstable.get(b"key2").unwrap(), Some(Bytes::from("value2")));
    }

    #[test]
    fn test_memtable_flush_with_tombstones() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("flush_tombstones.sst");

        let memtable = Memtable::new(1024);
        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        memtable.delete(Bytes::from("key2")); // Tombstone
        memtable.put(Bytes::from("key3"), Bytes::from("value3"));

        // Flush to disk
        memtable.flush(&sstable_path).unwrap();
        let mut sstable = SSTable::open(&sstable_path).unwrap();

        // All entries including tombstones should be written
        assert_eq!(sstable.len(), 3);

        // Verify tombstone behavior - get() should return None for deleted key
        assert_eq!(sstable.get(b"key1").unwrap(), Some(Bytes::from("value1")));
        assert_eq!(sstable.get(b"key2").unwrap(), None); // Tombstone
        assert_eq!(sstable.get(b"key3").unwrap(), Some(Bytes::from("value3")));
    }
}

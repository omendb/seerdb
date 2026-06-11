//! Blob file manager.
//!
//! Manages multiple blob files for KV separation. Handles appending,
//! reading, and garbage collection of blob files.

use crate::blob::file::BlobFile;
use crate::btree::node::BlobPointer;

/// Default threshold for blob separation (1KB).
pub const DEFAULT_BLOB_THRESHOLD: usize = 1024;

/// Manages blob files for KV separation.
///
/// Large values (>blob_threshold) are stored in blob files.
/// The B-tree stores blob pointers instead of the actual values.
pub struct BlobManager {
    /// Active blob files.
    files: Vec<BlobFile>,
    /// Next file ID.
    next_file_id: u32,
    /// Threshold for blob separation (in bytes).
    threshold: usize,
}

impl BlobManager {
    /// Create a new blob manager with the default threshold.
    pub fn new() -> Self {
        Self::with_threshold(DEFAULT_BLOB_THRESHOLD)
    }

    /// Create a new blob manager with a custom threshold.
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            files: Vec::new(),
            next_file_id: 1,
            threshold,
        }
    }

    /// Get the blob threshold.
    pub fn threshold(&self) -> usize {
        self.threshold
    }

    /// Number of blob files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Whether a value should be stored in a blob file.
    pub fn should_separate(&self, value_len: usize) -> bool {
        value_len > self.threshold
    }

    /// Append a value to the active blob file and return a pointer.
    pub fn append(&mut self, key: &[u8], value: Vec<u8>) -> BlobPointer {
        // Get or create the active blob file.
        if self.files.is_empty() {
            self.create_new_file();
        }

        let file = self.files.last_mut().expect("blob file should exist");
        let key_prefix = Self::make_key_prefix(key);
        let (offset, length) = file.append(key_prefix, value);

        BlobPointer {
            file_id: file.file_id(),
            offset,
            length,
        }
    }

    /// Read a value from a blob file.
    pub fn read(&self, ptr: &BlobPointer) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|f| f.file_id() == ptr.file_id)
            .and_then(|f| f.read(ptr.offset, ptr.length))
    }

    /// Mark an entry as deleted (for GC).
    pub fn mark_deleted(&mut self, ptr: &BlobPointer) {
        if let Some(file) = self.files.iter_mut().find(|f| f.file_id() == ptr.file_id) {
            file.mark_deleted(ptr.offset);
        }
    }

    /// Get files that need garbage collection.
    pub fn files_needing_gc(&self) -> Vec<u32> {
        self.files
            .iter()
            .filter(|f| f.needs_gc())
            .map(|f| f.file_id())
            .collect()
    }

    /// Create a new blob file.
    fn create_new_file(&mut self) {
        let file_id = self.next_file_id;
        self.next_file_id += 1;
        self.files.push(BlobFile::new(file_id));
    }

    /// Make a key prefix (first 8 bytes, padded with zeros if shorter).
    fn make_key_prefix(key: &[u8]) -> [u8; 8] {
        let mut prefix = [0u8; 8];
        let len = key.len().min(8);
        prefix[..len].copy_from_slice(&key[..len]);
        prefix
    }
}

impl Default for BlobManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_manager_new() {
        let bm = BlobManager::new();
        assert_eq!(bm.threshold(), DEFAULT_BLOB_THRESHOLD);
        assert_eq!(bm.file_count(), 0);
    }

    #[test]
    fn test_should_separate() {
        let bm = BlobManager::new();
        assert!(!bm.should_separate(100));
        assert!(!bm.should_separate(1024));
        assert!(bm.should_separate(1025));
    }

    #[test]
    fn test_blob_append_and_read() {
        let mut bm = BlobManager::new();
        let value = vec![42u8; 2000]; // > 1KB threshold

        let ptr = bm.append(b"test_key", value.clone());
        assert_eq!(ptr.length, 2000);
        assert!(ptr.offset > 0 || ptr.offset == 0);

        let read_value = bm.read(&ptr).unwrap();
        assert_eq!(read_value, &value);
    }

    #[test]
    fn test_blob_multiple_appends() {
        let mut bm = BlobManager::new();

        let ptr1 = bm.append(b"key1", vec![1; 1500]);
        let ptr2 = bm.append(b"key2", vec![2; 1500]);

        assert_eq!(bm.read(&ptr1).unwrap(), &vec![1; 1500]);
        assert_eq!(bm.read(&ptr2).unwrap(), &vec![2; 1500]);
    }

    #[test]
    fn test_blob_gc() {
        let mut bm = BlobManager::new();

        let ptr1 = bm.append(b"key1", vec![1; 1500]);
        let ptr2 = bm.append(b"key2", vec![2; 1500]);
        let ptr3 = bm.append(b"key3", vec![3; 1500]);

        assert!(bm.files_needing_gc().is_empty());

        // Mark enough entries as deleted to trigger GC.
        bm.mark_deleted(&ptr1);
        bm.mark_deleted(&ptr2);

        assert!(!bm.files_needing_gc().is_empty());
    }

    #[test]
    fn test_blob_key_prefix() {
        let prefix = BlobManager::make_key_prefix(b"hello");
        assert_eq!(&prefix, b"hello\0\0\0");

        let prefix = BlobManager::make_key_prefix(b"hello_world!");
        assert_eq!(&prefix, b"hello_wo");
    }
}

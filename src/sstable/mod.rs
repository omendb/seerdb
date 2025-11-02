// SSTable: Sorted String Table on disk
// Week 6: Enhanced with bloom filters, compression, and binary search
// Week 13: KV separation - stores value pointers for large values

use bytes::Bytes;
use crate::bloom::BloomFilter;
use crate::vlog::{ValuePointer, VLog};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SSTableError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Key not found")]
    KeyNotFound,

    #[error("Invalid SSTable format")]
    InvalidFormat,

    #[error("VLog error: {0}")]
    VLog(String),
}

pub type Result<T> = std::result::Result<T, SSTableError>;

/// Entry value - either inline or pointer to vLog
#[derive(Debug, Clone)]
enum EntryValue {
    Inline(Bytes),
    Pointer(ValuePointer),
}

/// SSTable writer with bloom filter support and optional KV separation
pub struct SSTableBuilder {
    entries: Vec<(Bytes, EntryValue)>,
    bloom: BloomFilter,
    vlog_threshold: Option<usize>, // If Some(n), values > n bytes go to vLog
}

impl SSTableBuilder {
    /// Create a new SSTable builder with default bloom filter (1% FPR, 10k elements)
    pub fn new() -> Self {
        Self::with_bloom_capacity(10000, 0.01)
    }

    /// Create a new SSTable builder with custom bloom filter parameters
    pub fn with_bloom_capacity(expected_elements: usize, false_positive_rate: f64) -> Self {
        Self {
            entries: Vec::new(),
            bloom: BloomFilter::new(expected_elements, false_positive_rate),
            vlog_threshold: None,
        }
    }

    /// Enable KV separation - values larger than threshold will be stored in vLog
    pub fn with_vlog_threshold(mut self, threshold: usize) -> Self {
        self.vlog_threshold = Some(threshold);
        self
    }

    /// Add a key-value pair (must be added in sorted order)
    /// If vLog is provided and value > threshold, stores value in vLog
    pub fn add(&mut self, key: Bytes, value: Bytes) {
        // Add key to bloom filter
        self.bloom.insert(&key);
        self.entries.push((key, EntryValue::Inline(value)));
    }

    /// Add a key-value pair with explicit vLog handling
    /// For large values, appends to vLog and stores pointer
    pub fn add_with_vlog(&mut self, key: Bytes, value: Bytes, vlog: &mut VLog) -> Result<()> {
        self.bloom.insert(&key);

        let entry_value = if let Some(threshold) = self.vlog_threshold {
            if value.len() > threshold {
                // Store in vLog, keep pointer
                let pointer = vlog
                    .append(&key, &value)
                    .map_err(|e| SSTableError::VLog(e.to_string()))?;
                EntryValue::Pointer(pointer)
            } else {
                // Store inline
                EntryValue::Inline(value)
            }
        } else {
            // No vLog, store inline
            EntryValue::Inline(value)
        };

        self.entries.push((key, entry_value));
        Ok(())
    }

    /// Build and write SSTable to disk
    /// Format: [entries...][index][bloom_filter_len: u64][bloom_filter][footer]
    /// Entry: [key_len: u32][key][flag: u8][value_data]
    ///   flag=0x00: inline  → [value_len: u32][value]
    ///   flag=0x01: pointer → [offset: u64][length: u32]
    /// Index: [num_entries: u32][(key_len: u32, key, offset: u64); num_entries]
    /// Footer: [index_offset: u64][bloom_offset: u64]
    pub fn build(self, path: impl AsRef<Path>) -> Result<SSTable> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;

        let mut index_entries = Vec::new();
        let mut current_offset = 0u64;

        // Write entries
        for (key, entry_value) in &self.entries {
            index_entries.push((key.clone(), current_offset));

            // Write key
            file.write_all(&(key.len() as u32).to_le_bytes())?;
            file.write_all(key)?;
            current_offset += 4 + key.len() as u64;

            // Write value (inline or pointer)
            match entry_value {
                EntryValue::Inline(value) => {
                    // Flag: 0x00 = inline
                    file.write_all(&[0x00])?;
                    current_offset += 1;

                    // Write value
                    file.write_all(&(value.len() as u32).to_le_bytes())?;
                    file.write_all(value)?;
                    current_offset += 4 + value.len() as u64;
                }
                EntryValue::Pointer(pointer) => {
                    // Flag: 0x01 = pointer
                    file.write_all(&[0x01])?;
                    current_offset += 1;

                    // Write pointer (offset + length)
                    file.write_all(&pointer.offset.to_le_bytes())?;
                    file.write_all(&pointer.length.to_le_bytes())?;
                    current_offset += 8 + 4;
                }
            }
        }

        let index_offset = current_offset;

        // Write index with keys for binary search
        file.write_all(&(index_entries.len() as u32).to_le_bytes())?;
        current_offset += 4;
        for (key, offset) in &index_entries {
            file.write_all(&(key.len() as u32).to_le_bytes())?;
            file.write_all(key)?;
            file.write_all(&offset.to_le_bytes())?;
            current_offset += 4 + key.len() as u64 + 8;
        }

        let bloom_offset = current_offset;

        // Write bloom filter
        let bloom_bytes = self.bloom.to_bytes();
        file.write_all(&(bloom_bytes.len() as u64).to_le_bytes())?;
        file.write_all(&bloom_bytes)?;

        // Write footer (index offset, bloom offset)
        file.write_all(&index_offset.to_le_bytes())?;
        file.write_all(&bloom_offset.to_le_bytes())?;

        file.sync_all()?;

        // Return SSTable reader
        SSTable::open(path)
    }
}

impl Default for SSTableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// SSTable reader with bloom filter and optional vLog
pub struct SSTable {
    file: File,
    path: PathBuf,
    index: Vec<(Bytes, u64)>,  // (Key, offset) pairs for binary search
    bloom: BloomFilter,
    num_entries: usize,
    vlog: Option<VLog>, // Optional vLog for reading value pointers
}

impl SSTable {
    /// Open an existing SSTable
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new().read(true).open(&path)?;

        // Read footer (last 16 bytes: index_offset + bloom_offset)
        file.seek(SeekFrom::End(-16))?;
        let mut footer_buf = [0u8; 16];
        file.read_exact(&mut footer_buf)?;
        let index_offset = u64::from_le_bytes(footer_buf[0..8].try_into().unwrap());
        let bloom_offset = u64::from_le_bytes(footer_buf[8..16].try_into().unwrap());

        // Read index
        file.seek(SeekFrom::Start(index_offset))?;

        let mut num_entries_buf = [0u8; 4];
        file.read_exact(&mut num_entries_buf)?;
        let num_entries = u32::from_le_bytes(num_entries_buf) as usize;

        let mut index = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            // Read key
            let mut key_len_buf = [0u8; 4];
            file.read_exact(&mut key_len_buf)?;
            let key_len = u32::from_le_bytes(key_len_buf) as usize;

            let mut key = vec![0u8; key_len];
            file.read_exact(&mut key)?;

            // Read offset
            let mut offset_buf = [0u8; 8];
            file.read_exact(&mut offset_buf)?;
            let offset = u64::from_le_bytes(offset_buf);

            index.push((Bytes::from(key), offset));
        }

        // Read bloom filter
        file.seek(SeekFrom::Start(bloom_offset))?;
        let mut bloom_len_buf = [0u8; 8];
        file.read_exact(&mut bloom_len_buf)?;
        let bloom_len = u64::from_le_bytes(bloom_len_buf) as usize;

        let mut bloom_bytes = vec![0u8; bloom_len];
        file.read_exact(&mut bloom_bytes)?;

        let bloom = BloomFilter::from_bytes(&bloom_bytes)
            .ok_or(SSTableError::InvalidFormat)?;

        Ok(Self {
            file,
            path,
            index,
            bloom,
            num_entries,
            vlog: None,
        })
    }

    /// Attach a vLog for reading value pointers
    pub fn with_vlog(mut self, vlog: VLog) -> Self {
        self.vlog = Some(vlog);
        self
    }

    /// Get a value by key using bloom filter + binary search
    /// Handles both inline values and vLog pointers
    pub fn get(&mut self, key: &[u8]) -> Result<Option<Bytes>> {
        // Check bloom filter first - if not present, definitely not in SSTable
        if !self.bloom.contains(&key) {
            return Ok(None);
        }

        // Bloom filter says key might be present, do binary search
        let result = self.index.binary_search_by(|(k, _)| k.as_ref().cmp(key));

        match result {
            Ok(idx) => {
                // Found the key, read the value
                let (_key, offset) = &self.index[idx];
                self.file.seek(SeekFrom::Start(*offset))?;

                // Read key (skip it)
                let mut key_len_buf = [0u8; 4];
                self.file.read_exact(&mut key_len_buf)?;
                let key_len = u32::from_le_bytes(key_len_buf) as usize;
                self.file.seek(SeekFrom::Current(key_len as i64))?;

                // Read flag byte
                let mut flag_buf = [0u8; 1];
                self.file.read_exact(&mut flag_buf)?;
                let flag = flag_buf[0];

                match flag {
                    0x00 => {
                        // Inline value
                        let mut value_len_buf = [0u8; 4];
                        self.file.read_exact(&mut value_len_buf)?;
                        let value_len = u32::from_le_bytes(value_len_buf) as usize;

                        let mut value = vec![0u8; value_len];
                        self.file.read_exact(&mut value)?;

                        Ok(Some(Bytes::from(value)))
                    }
                    0x01 => {
                        // Value pointer - read from vLog
                        let mut offset_buf = [0u8; 8];
                        self.file.read_exact(&mut offset_buf)?;
                        let vlog_offset = u64::from_le_bytes(offset_buf);

                        let mut length_buf = [0u8; 4];
                        self.file.read_exact(&mut length_buf)?;
                        let vlog_length = u32::from_le_bytes(length_buf);

                        let pointer = ValuePointer {
                            offset: vlog_offset,
                            length: vlog_length,
                        };

                        // Read from vLog
                        if let Some(vlog) = &mut self.vlog {
                            let value = vlog
                                .read(pointer)
                                .map_err(|e| SSTableError::VLog(e.to_string()))?;
                            Ok(Some(value))
                        } else {
                            Err(SSTableError::VLog(
                                "Value pointer found but no vLog attached".to_string(),
                            ))
                        }
                    }
                    _ => Err(SSTableError::InvalidFormat),
                }
            }
            Err(_) => {
                // Key not found (bloom filter false positive)
                Ok(None)
            }
        }
    }

    /// Iterate over all entries
    pub fn iter(&mut self) -> Result<SSTableIterator<'_>> {
        // Extract just the offsets for the iterator
        let offsets: Vec<u64> = self.index.iter().map(|(_, offset)| *offset).collect();
        SSTableIterator::new(&mut self.file, &offsets)
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.num_entries
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }

    /// Get file path
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Iterator over SSTable entries
pub struct SSTableIterator<'a> {
    file: &'a mut File,
    offsets: Vec<u64>,
    current_index: usize,
}

impl<'a> SSTableIterator<'a> {
    fn new(file: &'a mut File, offsets: &[u64]) -> Result<Self> {
        Ok(Self {
            file,
            offsets: offsets.to_vec(),
            current_index: 0,
        })
    }
}

impl<'a> Iterator for SSTableIterator<'a> {
    type Item = Result<(Bytes, Bytes)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.offsets.len() {
            return None;
        }

        let offset = self.offsets[self.current_index];
        self.current_index += 1;

        // Seek to entry
        if let Err(e) = self.file.seek(SeekFrom::Start(offset)) {
            return Some(Err(e.into()));
        }

        // Read key
        let mut key_len_buf = [0u8; 4];
        if let Err(e) = self.file.read_exact(&mut key_len_buf) {
            return Some(Err(e.into()));
        }
        let key_len = u32::from_le_bytes(key_len_buf) as usize;

        let mut key = vec![0u8; key_len];
        if let Err(e) = self.file.read_exact(&mut key) {
            return Some(Err(e.into()));
        }

        // Read flag byte
        let mut flag_buf = [0u8; 1];
        if let Err(e) = self.file.read_exact(&mut flag_buf) {
            return Some(Err(e.into()));
        }
        let flag = flag_buf[0];

        match flag {
            0x00 => {
                // Inline value
                let mut value_len_buf = [0u8; 4];
                if let Err(e) = self.file.read_exact(&mut value_len_buf) {
                    return Some(Err(e.into()));
                }
                let value_len = u32::from_le_bytes(value_len_buf) as usize;

                let mut value = vec![0u8; value_len];
                if let Err(e) = self.file.read_exact(&mut value) {
                    return Some(Err(e.into()));
                }

                Some(Ok((Bytes::from(key), Bytes::from(value))))
            }
            0x01 => {
                // Value pointer - skip for now (iterator doesn't have vLog access)
                // This is a limitation we'll address later
                Some(Err(SSTableError::VLog(
                    "Iterator doesn't support vLog pointers yet".to_string(),
                )))
            }
            _ => Some(Err(SSTableError::InvalidFormat)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_sstable_build_and_read() {
        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("test.sst");

        // Build SSTable
        let mut builder = SSTableBuilder::new();
        builder.add(Bytes::from("key1"), Bytes::from("value1"));
        builder.add(Bytes::from("key2"), Bytes::from("value2"));
        builder.add(Bytes::from("key3"), Bytes::from("value3"));

        let mut sstable = builder.build(&sstable_path).unwrap();

        assert_eq!(sstable.len(), 3);

        // Read values
        assert_eq!(
            sstable.get(b"key1").unwrap(),
            Some(Bytes::from("value1"))
        );
        assert_eq!(
            sstable.get(b"key2").unwrap(),
            Some(Bytes::from("value2"))
        );
        assert_eq!(
            sstable.get(b"key3").unwrap(),
            Some(Bytes::from("value3"))
        );
        assert_eq!(sstable.get(b"key4").unwrap(), None);
    }

    #[test]
    fn test_sstable_iterator() {
        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("test.sst");

        // Build SSTable
        let mut builder = SSTableBuilder::new();
        builder.add(Bytes::from("key1"), Bytes::from("value1"));
        builder.add(Bytes::from("key2"), Bytes::from("value2"));

        let mut sstable = builder.build(&sstable_path).unwrap();

        // Iterate
        let entries: Vec<_> = sstable
            .iter()
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, Bytes::from("key1"));
        assert_eq!(entries[1].0, Bytes::from("key2"));
    }

    #[test]
    fn test_sstable_reopen() {
        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("test.sst");

        // Build and close
        {
            let mut builder = SSTableBuilder::new();
            builder.add(Bytes::from("key1"), Bytes::from("value1"));
            builder.build(&sstable_path).unwrap();
        }

        // Reopen
        let mut sstable = SSTable::open(&sstable_path).unwrap();
        assert_eq!(
            sstable.get(b"key1").unwrap(),
            Some(Bytes::from("value1"))
        );
    }

    #[test]
    fn test_sstable_bloom_filter() {
        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("test_bloom.sst");

        // Build SSTable with 100 keys
        let mut builder = SSTableBuilder::with_bloom_capacity(100, 0.01);
        for i in 0..100 {
            let key = format!("key_{:03}", i);
            let value = format!("value_{}", i);
            builder.add(Bytes::from(key), Bytes::from(value));
        }

        let mut sstable = builder.build(&sstable_path).unwrap();

        // Keys that exist should be found
        assert_eq!(
            sstable.get(b"key_000").unwrap(),
            Some(Bytes::from("value_0"))
        );
        assert_eq!(
            sstable.get(b"key_050").unwrap(),
            Some(Bytes::from("value_50"))
        );
        assert_eq!(
            sstable.get(b"key_099").unwrap(),
            Some(Bytes::from("value_99"))
        );

        // Keys that don't exist should return None
        // Bloom filter should filter most of these without binary search
        assert_eq!(sstable.get(b"key_100").unwrap(), None);
        assert_eq!(sstable.get(b"key_999").unwrap(), None);
        assert_eq!(sstable.get(b"nonexistent").unwrap(), None);
    }

    #[test]
    fn test_sstable_with_vlog_inline_values() {
        use crate::vlog::VLog;

        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("test_vlog.sst");
        let vlog_path = dir.path().join("test.vlog");

        // Create vLog
        let mut vlog = VLog::create(&vlog_path).unwrap();

        // Build SSTable with small values (will be stored inline)
        let mut builder = SSTableBuilder::new().with_vlog_threshold(100); // 100 byte threshold
        builder.add_with_vlog(Bytes::from("key1"), Bytes::from("small_value"), &mut vlog).unwrap();
        builder.add_with_vlog(Bytes::from("key2"), Bytes::from("tiny"), &mut vlog).unwrap();

        let mut sstable = builder.build(&sstable_path).unwrap();

        // Read values - should work without vLog attached (inline values)
        assert_eq!(
            sstable.get(b"key1").unwrap(),
            Some(Bytes::from("small_value"))
        );
        assert_eq!(
            sstable.get(b"key2").unwrap(),
            Some(Bytes::from("tiny"))
        );
    }

    #[test]
    fn test_sstable_with_vlog_large_values() {
        use crate::vlog::VLog;

        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("test_vlog.sst");
        let vlog_path = dir.path().join("test.vlog");

        // Create vLog
        let mut vlog = VLog::create(&vlog_path).unwrap();

        // Build SSTable with large values (will be stored in vLog)
        let mut builder = SSTableBuilder::new().with_vlog_threshold(10); // 10 byte threshold
        let large_value = vec![b'x'; 100];
        builder.add_with_vlog(Bytes::from("key1"), Bytes::from(large_value.clone()), &mut vlog).unwrap();

        let mut sstable = builder.build(&sstable_path).unwrap();

        // Try to read without vLog - should fail
        let result = sstable.get(b"key1");
        assert!(result.is_err());

        // Reopen vLog and attach to SSTable
        let vlog = VLog::open(&vlog_path).unwrap();
        let mut sstable = SSTable::open(&sstable_path).unwrap().with_vlog(vlog);

        // Now should work
        assert_eq!(
            sstable.get(b"key1").unwrap(),
            Some(Bytes::from(large_value))
        );
    }

    #[test]
    fn test_sstable_with_vlog_mixed_values() {
        use crate::vlog::VLog;

        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("test_vlog.sst");
        let vlog_path = dir.path().join("test.vlog");

        // Create vLog
        let mut vlog = VLog::create(&vlog_path).unwrap();

        // Build SSTable with mixed small and large values
        let mut builder = SSTableBuilder::new().with_vlog_threshold(50); // 50 byte threshold
        builder.add_with_vlog(Bytes::from("key1"), Bytes::from("small"), &mut vlog).unwrap();
        let large_value = vec![b'x'; 100];
        builder.add_with_vlog(Bytes::from("key2"), Bytes::from(large_value.clone()), &mut vlog).unwrap();
        builder.add_with_vlog(Bytes::from("key3"), Bytes::from("also_small"), &mut vlog).unwrap();

        let mut sstable = builder.build(&sstable_path).unwrap();

        // Small values work without vLog
        assert_eq!(
            sstable.get(b"key1").unwrap(),
            Some(Bytes::from("small"))
        );
        assert_eq!(
            sstable.get(b"key3").unwrap(),
            Some(Bytes::from("also_small"))
        );

        // Large value requires vLog
        let result = sstable.get(b"key2");
        assert!(result.is_err());

        // Attach vLog
        let vlog = VLog::open(&vlog_path).unwrap();
        let mut sstable = SSTable::open(&sstable_path).unwrap().with_vlog(vlog);

        // All values should work now
        assert_eq!(
            sstable.get(b"key1").unwrap(),
            Some(Bytes::from("small"))
        );
        assert_eq!(
            sstable.get(b"key2").unwrap(),
            Some(Bytes::from(large_value))
        );
        assert_eq!(
            sstable.get(b"key3").unwrap(),
            Some(Bytes::from("also_small"))
        );
    }

    #[test]
    fn test_sstable_vlog_reopen() {
        use crate::vlog::VLog;

        let dir = tempdir().unwrap();
        let sstable_path = dir.path().join("test_vlog.sst");
        let vlog_path = dir.path().join("test.vlog");

        // Create and populate
        {
            let mut vlog = VLog::create(&vlog_path).unwrap();
            let mut builder = SSTableBuilder::new().with_vlog_threshold(20);

            let large_value = vec![b'A'; 100];
            builder.add_with_vlog(Bytes::from("key1"), Bytes::from(large_value), &mut vlog).unwrap();
            builder.build(&sstable_path).unwrap();
        }

        // Reopen and verify
        let vlog = VLog::open(&vlog_path).unwrap();
        let mut sstable = SSTable::open(&sstable_path).unwrap().with_vlog(vlog);

        let value = sstable.get(b"key1").unwrap().unwrap();
        assert_eq!(value.len(), 100);
        assert_eq!(value[0], b'A');
    }
}

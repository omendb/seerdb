// SSTable: Sorted String Table on disk
// Week 6: Enhanced with bloom filters, compression, and binary search

use bytes::Bytes;
use crate::bloom::BloomFilter;
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
}

pub type Result<T> = std::result::Result<T, SSTableError>;

/// SSTable writer with bloom filter support
pub struct SSTableBuilder {
    entries: Vec<(Bytes, Bytes)>,
    bloom: BloomFilter,
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
        }
    }

    /// Add a key-value pair (must be added in sorted order)
    pub fn add(&mut self, key: Bytes, value: Bytes) {
        // Add key to bloom filter
        self.bloom.insert(&key);
        self.entries.push((key, value));
    }

    /// Build and write SSTable to disk
    /// Format: [entries...][index][bloom_filter_len: u64][bloom_filter][footer]
    /// Entry: [key_len: u32][key][value_len: u32][value]
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
        for (key, value) in &self.entries {
            index_entries.push((key.clone(), current_offset));

            // Write key
            file.write_all(&(key.len() as u32).to_le_bytes())?;
            file.write_all(key)?;
            current_offset += 4 + key.len() as u64;

            // Write value
            file.write_all(&(value.len() as u32).to_le_bytes())?;
            file.write_all(value)?;
            current_offset += 4 + value.len() as u64;
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

/// SSTable reader with bloom filter
pub struct SSTable {
    file: File,
    path: PathBuf,
    index: Vec<(Bytes, u64)>,  // (Key, offset) pairs for binary search
    bloom: BloomFilter,
    num_entries: usize,
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
        })
    }

    /// Get a value by key using bloom filter + binary search
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

                // Read value
                let mut value_len_buf = [0u8; 4];
                self.file.read_exact(&mut value_len_buf)?;
                let value_len = u32::from_le_bytes(value_len_buf) as usize;

                let mut value = vec![0u8; value_len];
                self.file.read_exact(&mut value)?;

                Ok(Some(Bytes::from(value)))
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

        // Read value
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
}

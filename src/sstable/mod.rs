// SSTable: Sorted String Table on disk
// Simple implementation for Week 5 (flush memtable to disk)
// Week 6 will add: bloom filters, compression, learned index

use bytes::{Bytes, BytesMut, Buf, BufMut};
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

/// Simple SSTable writer (Week 5 version)
pub struct SSTableBuilder {
    entries: Vec<(Bytes, Bytes)>,
}

impl SSTableBuilder {
    /// Create a new SSTable builder
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a key-value pair (must be added in sorted order)
    pub fn add(&mut self, key: Bytes, value: Bytes) {
        self.entries.push((key, value));
    }

    /// Build and write SSTable to disk
    /// Format: [entries...][index][footer]
    /// Entry: [key_len: u32][key][value_len: u32][value]
    /// Index: [num_entries: u32][offsets: [u64; num_entries]]
    /// Footer: [index_offset: u64]
    pub fn build(self, path: impl AsRef<Path>) -> Result<SSTable> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;

        let mut offsets = Vec::new();
        let mut current_offset = 0u64;

        // Write entries
        for (key, value) in &self.entries {
            offsets.push(current_offset);

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

        // Write index
        file.write_all(&(offsets.len() as u32).to_le_bytes())?;
        for offset in &offsets {
            file.write_all(&offset.to_le_bytes())?;
        }

        // Write footer (index offset)
        file.write_all(&index_offset.to_le_bytes())?;

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

/// SSTable reader
pub struct SSTable {
    file: File,
    path: PathBuf,
    index: Vec<u64>,  // Offsets of each entry
    num_entries: usize,
}

impl SSTable {
    /// Open an existing SSTable
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new().read(true).open(&path)?;

        // Read footer (last 8 bytes)
        file.seek(SeekFrom::End(-8))?;
        let mut footer_buf = [0u8; 8];
        file.read_exact(&mut footer_buf)?;
        let index_offset = u64::from_le_bytes(footer_buf);

        // Read index
        file.seek(SeekFrom::Start(index_offset))?;

        let mut num_entries_buf = [0u8; 4];
        file.read_exact(&mut num_entries_buf)?;
        let num_entries = u32::from_le_bytes(num_entries_buf) as usize;

        let mut index = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            let mut offset_buf = [0u8; 8];
            file.read_exact(&mut offset_buf)?;
            index.push(u64::from_le_bytes(offset_buf));
        }

        Ok(Self {
            file,
            path,
            index,
            num_entries,
        })
    }

    /// Get a value by key (linear search for now, Week 6 will add binary search + bloom filter)
    pub fn get(&mut self, key: &[u8]) -> Result<Option<Bytes>> {
        for &offset in &self.index {
            self.file.seek(SeekFrom::Start(offset))?;

            // Read key
            let mut key_len_buf = [0u8; 4];
            self.file.read_exact(&mut key_len_buf)?;
            let key_len = u32::from_le_bytes(key_len_buf) as usize;

            let mut entry_key = vec![0u8; key_len];
            self.file.read_exact(&mut entry_key)?;

            // Check if this is the key we're looking for
            if entry_key == key {
                // Read value
                let mut value_len_buf = [0u8; 4];
                self.file.read_exact(&mut value_len_buf)?;
                let value_len = u32::from_le_bytes(value_len_buf) as usize;

                let mut value = vec![0u8; value_len];
                self.file.read_exact(&mut value)?;

                return Ok(Some(Bytes::from(value)));
            }

            // Skip value if not the key we want
            let mut value_len_buf = [0u8; 4];
            self.file.read_exact(&mut value_len_buf)?;
            let value_len = u32::from_le_bytes(value_len_buf) as usize;
            self.file.seek(SeekFrom::Current(value_len as i64))?;
        }

        Ok(None)
    }

    /// Iterate over all entries
    pub fn iter(&mut self) -> Result<SSTableIterator> {
        SSTableIterator::new(&mut self.file, &self.index)
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
}

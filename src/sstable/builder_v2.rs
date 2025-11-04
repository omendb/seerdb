// Block-based SSTable Builder (V2 Format)
// Writes data incrementally in blocks without buffering all entries in memory

use super::block::{BlockBuilder, DEFAULT_BLOCK_SIZE};
use crate::bloom::BloomFilter;
use crate::vlog::VLog;
use bytes::{Bytes, BytesMut};
use crc32fast::Hasher;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::{SSTableError, Result};

/// Magic number for SSTable V2 format: "SSTB"
const MAGIC_V2: u32 = 0x53535442;
const VERSION_V2: u32 = 0x00000002;

/// Entry value type flag
const FLAG_INLINE: u8 = 0x00;
const FLAG_POINTER: u8 = 0x01;

/// Top-level index entry: (last_key_in_block, block_offset)
#[derive(Debug, Clone)]
struct TopLevelIndexEntry {
    last_key: Bytes,
    offset: u64,
}

/// SSTable builder V2 with block-based format
pub struct SSTableBuilderV2 {
    /// Output file
    file: File,
    /// Current data block builder
    data_block: BlockBuilder,
    /// Current index block builder
    index_block: BlockBuilder,
    /// Top-level index (kept in memory during build)
    top_level_index: Vec<TopLevelIndexEntry>,
    /// Bloom filter
    bloom: BloomFilter,
    /// VLog threshold
    vlog_threshold: Option<usize>,
    /// Total entries added
    num_entries: u64,
    /// Current file offset
    current_offset: u64,
    /// First data block offset (after header)
    data_blocks_start: u64,
    /// Index blocks start offset
    index_blocks_start: u64,
}

impl SSTableBuilderV2 {
    /// Create a new V2 SSTable builder
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)   // Need read for checksum calculation
            .write(true)
            .truncate(true)
            .open(path)?;

        // Write file header
        let header = Self::create_header();
        file.write_all(&header)?;
        let header_size = header.len() as u64;

        Ok(Self {
            file,
            data_block: BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE),
            index_block: BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE),
            top_level_index: Vec::new(),
            bloom: BloomFilter::new(10000, 0.01),
            vlog_threshold: None,
            num_entries: 0,
            current_offset: header_size,
            data_blocks_start: header_size,
            index_blocks_start: 0,
        })
    }

    /// Set vlog threshold
    pub fn with_vlog_threshold(mut self, threshold: usize) -> Self {
        self.vlog_threshold = Some(threshold);
        self
    }

    /// Add an entry (inline value)
    pub fn add(&mut self, key: Bytes, value: Bytes) -> Result<()> {
        self.bloom.insert(&key);

        // Encode entry
        let entry = self.encode_entry(&key, FLAG_INLINE, &value);

        // Try to add to current data block
        if !self.data_block.add(&key, &entry) {
            // Block full, flush it
            self.flush_data_block()?;

            // Try again with new block (should succeed)
            if !self.data_block.add(&key, &entry) {
                return Err(SSTableError::InvalidFormat);
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    /// Add entry with vlog support
    pub fn add_with_vlog(&mut self, key: Bytes, value: Bytes, vlog: &mut VLog) -> Result<()> {
        self.bloom.insert(&key);

        // Determine if value should go to vlog
        let (flag, data) = if let Some(threshold) = self.vlog_threshold {
            if value.len() > threshold {
                // Write to vlog
                let pointer = vlog
                    .append(&key, &value)
                    .map_err(|e| SSTableError::VLog(e.to_string()))?;

                // Encode pointer
                let mut ptr_data = BytesMut::with_capacity(12);
                ptr_data.extend_from_slice(&pointer.offset.to_le_bytes());
                ptr_data.extend_from_slice(&pointer.length.to_le_bytes());

                (FLAG_POINTER, ptr_data.freeze())
            } else {
                (FLAG_INLINE, value)
            }
        } else {
            (FLAG_INLINE, value)
        };

        // Encode entry
        let entry = self.encode_entry(&key, flag, &data);

        // Try to add to current data block
        if !self.data_block.add(&key, &entry) {
            // Block full, flush it
            self.flush_data_block()?;

            // Try again with new block
            if !self.data_block.add(&key, &entry) {
                return Err(SSTableError::InvalidFormat);
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    /// Encode an entry: [flag: u8][data: bytes]
    fn encode_entry(&self, _key: &[u8], flag: u8, data: &[u8]) -> Bytes {
        let mut buf = BytesMut::with_capacity(1 + data.len());
        buf.extend_from_slice(&[flag]);
        buf.extend_from_slice(data);
        buf.freeze()
    }

    /// Flush current data block and add to index
    fn flush_data_block(&mut self) -> Result<()> {
        if self.data_block.is_empty() {
            return Ok(());
        }

        let last_key = Bytes::copy_from_slice(self.data_block.last_key());
        let block_offset = self.current_offset;

        // Write data block (take ownership with replace, finish, then create new)
        let old_block = std::mem::replace(&mut self.data_block, BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE));
        let block_data = old_block.finish();
        self.file.write_all(&block_data)?;
        self.current_offset += block_data.len() as u64;

        // Build index entry: [key_len][key][offset: u64]
        let mut index_entry = BytesMut::with_capacity(4 + last_key.len() + 8);
        index_entry.extend_from_slice(&(last_key.len() as u32).to_le_bytes());
        index_entry.extend_from_slice(&last_key);
        index_entry.extend_from_slice(&block_offset.to_le_bytes());
        let index_entry_bytes = index_entry.freeze();

        // Try to add to index block
        if !self.index_block.add(&last_key, &index_entry_bytes) {
            // Index block full, flush it
            self.flush_index_block()?;

            // Add to new index block (rebuild entry since we consumed it)
            let mut index_entry2 = BytesMut::with_capacity(4 + last_key.len() + 8);
            index_entry2.extend_from_slice(&(last_key.len() as u32).to_le_bytes());
            index_entry2.extend_from_slice(&last_key);
            index_entry2.extend_from_slice(&block_offset.to_le_bytes());

            if !self.index_block.add(&last_key, &index_entry2.freeze()) {
                return Err(SSTableError::InvalidFormat);
            }
        }

        Ok(())
    }

    /// Flush current index block and add to top-level index
    fn flush_index_block(&mut self) -> Result<()> {
        if self.index_block.is_empty() {
            return Ok(());
        }

        // Mark start of index blocks section (first flush)
        if self.index_blocks_start == 0 {
            self.index_blocks_start = self.current_offset;
        }

        let last_key = Bytes::copy_from_slice(self.index_block.last_key());
        let block_offset = self.current_offset;

        // Write index block (take ownership with replace)
        let old_block = std::mem::replace(&mut self.index_block, BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE));
        let block_data = old_block.finish();
        self.file.write_all(&block_data)?;
        self.current_offset += block_data.len() as u64;

        // Add to top-level index
        self.top_level_index.push(TopLevelIndexEntry {
            last_key,
            offset: block_offset,
        });

        Ok(())
    }

    /// Finish building and write metadata
    pub fn finish(mut self) -> Result<()> {
        // Flush any remaining data block
        self.flush_data_block()?;

        // Flush any remaining index block
        self.flush_index_block()?;

        // Write top-level index
        let top_level_offset = self.current_offset;
        self.write_top_level_index()?;

        // Write bloom filter
        let bloom_offset = self.current_offset;
        let bloom_bytes = self.bloom.to_bytes();
        self.file.write_all(&(bloom_bytes.len() as u64).to_le_bytes())?;
        self.file.write_all(&bloom_bytes)?;
        self.current_offset += 8 + bloom_bytes.len() as u64;

        // Write footer
        self.write_footer(top_level_offset, bloom_offset)?;

        // Update header with final entry count
        self.file.seek(SeekFrom::Start(8))?; // Skip magic + version
        self.file.write_all(&0u64.to_le_bytes())?; // flags
        self.file.write_all(&self.num_entries.to_le_bytes())?;

        self.file.sync_all()?;
        Ok(())
    }

    /// Write top-level index
    fn write_top_level_index(&mut self) -> Result<()> {
        // Number of index blocks
        self.file.write_all(&(self.top_level_index.len() as u32).to_le_bytes())?;
        self.current_offset += 4;

        // Each entry: [key_len][key][offset]
        for entry in &self.top_level_index {
            self.file.write_all(&(entry.last_key.len() as u32).to_le_bytes())?;
            self.file.write_all(&entry.last_key)?;
            self.file.write_all(&entry.offset.to_le_bytes())?;
            self.current_offset += 4 + entry.last_key.len() as u64 + 8;
        }

        Ok(())
    }

    /// Write footer (48 bytes)
    fn write_footer(&mut self, top_level_offset: u64, bloom_offset: u64) -> Result<()> {
        let footer_start = self.current_offset;

        // Calculate checksum over entire file except footer checksum field
        self.file.seek(SeekFrom::Start(0))?;
        let mut hasher = Hasher::new();
        let mut buf = vec![0u8; 4096];
        let mut remaining = footer_start;

        while remaining > 0 {
            let to_read = remaining.min(4096) as usize;
            self.file.read_exact(&mut buf[..to_read])?;
            hasher.update(&buf[..to_read]);
            remaining -= to_read as u64;
        }

        let checksum = hasher.finalize();

        // Seek to end to write footer
        self.file.seek(SeekFrom::Start(footer_start))?;

        self.file.write_all(&self.index_blocks_start.to_le_bytes())?;
        self.file.write_all(&top_level_offset.to_le_bytes())?;
        self.file.write_all(&bloom_offset.to_le_bytes())?;
        self.file.write_all(&checksum.to_le_bytes())?;
        self.file.write_all(&MAGIC_V2.to_le_bytes())?;
        self.file.write_all(&VERSION_V2.to_le_bytes())?;
        self.file.write_all(&0u32.to_le_bytes())?; // reserved

        Ok(())
    }

    /// Create file header (32 bytes)
    fn create_header() -> Vec<u8> {
        let mut header = Vec::with_capacity(32);
        header.extend_from_slice(&MAGIC_V2.to_le_bytes());
        header.extend_from_slice(&VERSION_V2.to_le_bytes());
        header.extend_from_slice(&0u64.to_le_bytes()); // flags
        header.extend_from_slice(&0u64.to_le_bytes()); // num_entries (updated at end)
        header.extend_from_slice(&0u64.to_le_bytes()); // reserved
        header
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_builder_v2_simple() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        let mut builder = SSTableBuilderV2::create(&path).unwrap();
        builder.add(Bytes::from("key1"), Bytes::from("value1")).unwrap();
        builder.add(Bytes::from("key2"), Bytes::from("value2")).unwrap();
        builder.add(Bytes::from("key3"), Bytes::from("value3")).unwrap();
        builder.finish().unwrap();

        // Verify file exists and has content
        assert!(path.exists());
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_builder_v2_many_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        let mut builder = SSTableBuilderV2::create(&path).unwrap();

        // Add enough entries to span multiple blocks
        for i in 0..1000 {
            let key = Bytes::from(format!("key{:06}", i));
            let value = Bytes::from(format!("value{:06}", i));
            builder.add(key, value).unwrap();
        }

        builder.finish().unwrap();

        assert!(path.exists());
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 4096); // Should have multiple blocks
    }
}

// SSTable: Sorted String Table on disk
// Block-based format with lazy loading for memory efficiency

pub mod block;

use crate::alex::AlexTree;
use crate::bloom::BloomFilter;
use crate::vlog::{VLog, ValuePointer};
use block::{Block, BlockBuilder, BlockError, DEFAULT_BLOCK_SIZE};
use bytes::{Bytes, BytesMut};
use quick_cache::sync::Cache;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
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

    #[error("SSTable corrupted: expected checksum {expected:#x}, got {actual:#x}")]
    Corruption { expected: u32, actual: u32 },

    #[error("Block error: {0}")]
    Block(#[from] BlockError),
}

pub type Result<T> = std::result::Result<T, SSTableError>;

/// Magic number for SSTable format: "SSTB"
const MAGIC: u32 = 0x53535442;
const VERSION: u32 = 0x00000001; // v1: Format versions don't matter until we release

/// Entry value type flags
pub const FLAG_INLINE: u8 = 0x00;
pub const FLAG_POINTER: u8 = 0x01;
pub const FLAG_TOMBSTONE: u8 = 0x02;

/// Helper: Handle vLog separation logic (shared by both SSTableBuilder and BufferedSSTableBuilder)
///
/// Returns (encoded_value, flag):
/// - If value is large (>threshold), appends to vLog and returns (pointer_bytes, FLAG_POINTER)
/// - Otherwise, returns (value, FLAG_INLINE)
fn handle_vlog_value(
    key: &Bytes,
    value: Bytes,
    vlog: &mut VLog,
    threshold: Option<usize>,
) -> Result<(Bytes, u8)> {
    if value.len() > threshold.unwrap_or(usize::MAX) {
        // Large value: store in vLog and return pointer
        let pointer = vlog.append(key, &value).map_err(|e| {
            SSTableError::VLog(format!("Failed to append to vLog: {}", e))
        })?;
        Ok((pointer.to_bytes(), FLAG_POINTER))
    } else {
        // Small value: store inline
        Ok((value, FLAG_INLINE))
    }
}

/// Top-level index entry (loaded into RAM)
#[derive(Debug, Clone)]
struct TopLevelIndexEntry {
    last_key: Bytes,
    offset: u64,
    size: u32,
}

/// Convert bytes key to i64 for ALEX index
/// Uses big-endian to preserve lexicographic ordering
fn bytes_to_i64(bytes: &[u8]) -> i64 {
    // Convert key bytes to i64 while preserving lexicographic ordering
    // Bug #11 fix: Previous implementation only used first 8 bytes, causing collisions
    // when keys share the same prefix (e.g., "key_0000000000" and "key_0000000100" both
    // had "key_0000" as first 8 bytes, hashing to the same value)
    //
    // New approach: Use bytes at strategic positions to capture differences
    // Position 0, 2, 4, 6, 8, 10, 12, len-1 gives good spread while maintaining some ordering

    let len = bytes.len();
    if len <= 8 {
        // Short keys: use all bytes with padding
        let mut buf = [0u8; 8];
        buf[..len].copy_from_slice(&bytes[..len]);
        i64::from_be_bytes(buf)
    } else {
        // Long keys: sample bytes at multiple positions to capture structure
        // This provides better collision resistance than first/last only
        let positions = [
            0,
            2,
            4,
            6,
            8.min(len - 1),
            10.min(len - 1),
            12.min(len - 1),
            len - 1,
        ];
        let mut buf = [0u8; 8];
        for (i, &pos) in positions.iter().enumerate() {
            if pos < len {
                buf[i] = bytes[pos];
            }
        }
        i64::from_be_bytes(buf)
    }
}

// ============================================================================
// SSTableBuilder - Write SSTables incrementally
// ============================================================================

/// SSTable builder with block-based format
///
/// TODO (Phase 2 - Object Storage): SSTableBuilder currently uses File directly
/// for streaming writes (write header, blocks incrementally, footer). When adding
/// object storage backends (S3/GCS/Azure), we'll need to:
/// 1. Buffer all SSTable data in memory
/// 2. Call LocalStorage::write_sstable() once at the end with complete data
/// 3. For S3, this maps directly to PutObject operation
///
/// This is deferred to Phase 2 (post-0.0.1) to avoid increasing memory usage
/// and complexity during production hardening.
pub struct SSTableBuilder {
    file: File,
    data_block: BlockBuilder,
    index_block: BlockBuilder,
    top_level_index: Vec<TopLevelIndexEntry>,
    bloom: BloomFilter,
    vlog_threshold: Option<usize>,
    num_entries: u64,
    current_offset: u64,
    index_blocks_start: u64,
    min_key: Option<Bytes>,
    max_key: Option<Bytes>,
    /// Maximum sequence number in this SSTable
    /// Used to coordinate flush and compaction to prevent live key deletion
    max_sequence: u64,
}

impl SSTableBuilder {
    /// Create a new SSTable builder
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(path)?;

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
            index_blocks_start: 0,
            min_key: None,
            max_key: None,
            max_sequence: 0,
        })
    }

    pub fn with_vlog_threshold(mut self, threshold: usize) -> Self {
        self.vlog_threshold = Some(threshold);
        self
    }

    /// Set the maximum sequence number for this SSTable
    /// Should be called before finish() to track flush/compaction coordination
    pub fn with_max_sequence(mut self, seq: u64) -> Self {
        self.max_sequence = seq;
        self
    }

    pub fn add(&mut self, key: Bytes, value: Bytes) -> Result<()> {
        // Track min/max keys for range filtering
        if self.min_key.is_none() {
            self.min_key = Some(key.clone());
        }
        self.max_key = Some(key.clone());

        self.bloom.insert(&key);
        let entry = self.encode_entry(&key, FLAG_INLINE, &value);

        if !self.data_block.add(&key, &entry) {
            self.flush_data_block()?;
            if !self.data_block.add(&key, &entry) {
                // Entry too large for default block - create custom-sized block
                let entry_size = key.len() + entry.len() + 8; // key + entry + headers
                let custom_size = (entry_size * 2).max(DEFAULT_BLOCK_SIZE * 2); // 2x for safety
                self.data_block = BlockBuilder::with_capacity(custom_size);

                if !self.data_block.add(&key, &entry) {
                    return Err(SSTableError::InvalidFormat);
                }
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    /// Add a raw entry that already has a flag prefix (for compaction)
    /// This preserves the original encoding (FLAG_INLINE, FLAG_POINTER, etc.)
    pub fn add_raw(&mut self, key: Bytes, encoded_value: Bytes) -> Result<()> {
        // Track min/max keys for range filtering
        if self.min_key.is_none() {
            self.min_key = Some(key.clone());
        }
        self.max_key = Some(key.clone());

        self.bloom.insert(&key);

        if !self.data_block.add(&key, &encoded_value) {
            self.flush_data_block()?;
            if !self.data_block.add(&key, &encoded_value) {
                // Entry too large for default block - create custom-sized block
                let entry_size = key.len() + encoded_value.len() + 8; // key + value + headers
                let custom_size = (entry_size * 2).max(DEFAULT_BLOCK_SIZE * 2); // 2x for safety
                self.data_block = BlockBuilder::with_capacity(custom_size);

                if !self.data_block.add(&key, &encoded_value) {
                    return Err(SSTableError::InvalidFormat);
                }
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    pub fn add_with_vlog(&mut self, key: Bytes, value: Bytes, vlog: &mut VLog) -> Result<()> {
        self.bloom.insert(&key);

        // Use shared helper for vLog handling
        let (data, flag) = handle_vlog_value(&key, value, vlog, self.vlog_threshold)?;

        let entry = self.encode_entry(&key, flag, &data);

        if !self.data_block.add(&key, &entry) {
            self.flush_data_block()?;
            if !self.data_block.add(&key, &entry) {
                // Entry too large for default block - create custom-sized block
                let entry_size = key.len() + entry.len() + 8; // key + entry + headers
                let custom_size = (entry_size * 2).max(DEFAULT_BLOCK_SIZE * 2); // 2x for safety
                self.data_block = BlockBuilder::with_capacity(custom_size);

                if !self.data_block.add(&key, &entry) {
                    return Err(SSTableError::InvalidFormat);
                }
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    pub fn add_tombstone(&mut self, key: Bytes) -> Result<()> {
        self.bloom.insert(&key);
        let entry = self.encode_entry(&key, FLAG_TOMBSTONE, &[]);

        if !self.data_block.add(&key, &entry) {
            self.flush_data_block()?;
            if !self.data_block.add(&key, &entry) {
                // Entry too large for default block - create custom-sized block
                let entry_size = key.len() + entry.len() + 8; // key + entry + headers
                let custom_size = (entry_size * 2).max(DEFAULT_BLOCK_SIZE * 2); // 2x for safety
                self.data_block = BlockBuilder::with_capacity(custom_size);

                if !self.data_block.add(&key, &entry) {
                    return Err(SSTableError::InvalidFormat);
                }
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    fn encode_entry(&self, _key: &[u8], flag: u8, data: &[u8]) -> Bytes {
        let mut buf = BytesMut::with_capacity(1 + data.len());
        buf.extend_from_slice(&[flag]);
        buf.extend_from_slice(data);
        buf.freeze()
    }

    fn flush_data_block(&mut self) -> Result<()> {
        if self.data_block.is_empty() {
            return Ok(());
        }

        let last_key = Bytes::copy_from_slice(self.data_block.last_key());
        let block_offset = self.current_offset;

        let old_block = std::mem::replace(
            &mut self.data_block,
            BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE),
        );
        let block_data = old_block.finish();
        let block_size = block_data.len() as u32;
        self.file.write_all(&block_data)?;
        self.current_offset += block_data.len() as u64;

        let mut index_entry = BytesMut::with_capacity(4 + last_key.len() + 8 + 4);
        index_entry.extend_from_slice(&(last_key.len() as u32).to_le_bytes());
        index_entry.extend_from_slice(&last_key);
        index_entry.extend_from_slice(&block_offset.to_le_bytes());
        index_entry.extend_from_slice(&block_size.to_le_bytes());
        let index_entry_bytes = index_entry.freeze();

        if !self.index_block.add(&last_key, &index_entry_bytes) {
            self.flush_index_block()?;

            let mut index_entry2 = BytesMut::with_capacity(4 + last_key.len() + 8 + 4);
            index_entry2.extend_from_slice(&(last_key.len() as u32).to_le_bytes());
            index_entry2.extend_from_slice(&last_key);
            index_entry2.extend_from_slice(&block_offset.to_le_bytes());
            index_entry2.extend_from_slice(&block_size.to_le_bytes());

            if !self.index_block.add(&last_key, &index_entry2.freeze()) {
                return Err(SSTableError::InvalidFormat);
            }
        }

        Ok(())
    }

    fn flush_index_block(&mut self) -> Result<()> {
        if self.index_block.is_empty() {
            return Ok(());
        }

        if self.index_blocks_start == 0 {
            self.index_blocks_start = self.current_offset;
        }

        let last_key = Bytes::copy_from_slice(self.index_block.last_key());
        let block_offset = self.current_offset;

        let old_block = std::mem::replace(
            &mut self.index_block,
            BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE),
        );
        let block_data = old_block.finish();
        let block_size = block_data.len() as u32;
        self.file.write_all(&block_data)?;
        self.current_offset += block_data.len() as u64;

        self.top_level_index.push(TopLevelIndexEntry {
            last_key,
            offset: block_offset,
            size: block_size,
        });

        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.flush_data_block()?;
        self.flush_index_block()?;

        let top_level_offset = self.current_offset;
        self.write_top_level_index()?;

        let bloom_offset = self.current_offset;
        let bloom_bytes = self.bloom.to_bytes();
        self.file
            .write_all(&(bloom_bytes.len() as u64).to_le_bytes())?;
        self.file.write_all(&bloom_bytes)?;
        self.current_offset += 8 + bloom_bytes.len() as u64;

        // Write min_key/max_key metadata
        let metadata_offset = self.current_offset;
        self.write_metadata()?;

        // Write num_entries and max_sequence to header BEFORE computing footer checksum
        // This ensures the checksum includes these header fields
        let footer_offset = self.current_offset;
        self.file.seek(SeekFrom::Start(16))?; // Skip magic (4) + version (4) + reserved (8)
        self.file.write_all(&self.num_entries.to_le_bytes())?; // Offset 16-23
        self.file.write_all(&self.max_sequence.to_le_bytes())?; // Offset 24-31
        self.file.seek(SeekFrom::Start(footer_offset))?; // Return to footer position

        self.write_footer(top_level_offset, bloom_offset, metadata_offset)?;

        // CRITICAL: Fsync to ensure durability (sync data + metadata)
        // This guarantees all SSTable data is persisted to disk before returning
        self.file.sync_all()?;
        Ok(())
    }

    fn write_top_level_index(&mut self) -> Result<()> {
        // OPTIMIZATION: Batch all index entries into single buffer to reduce syscalls
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&(self.top_level_index.len() as u32).to_le_bytes());

        for entry in &self.top_level_index {
            buffer.extend_from_slice(&(entry.last_key.len() as u32).to_le_bytes());
            buffer.extend_from_slice(&entry.last_key);
            buffer.extend_from_slice(&entry.offset.to_le_bytes());
            buffer.extend_from_slice(&entry.size.to_le_bytes());
        }

        // Single syscall instead of N syscalls
        self.file.write_all(&buffer)?;
        self.current_offset += buffer.len() as u64;

        Ok(())
    }

    fn write_metadata(&mut self) -> Result<()> {
        // OPTIMIZATION: Batch metadata writes into single buffer
        let mut buffer = Vec::new();

        // Write min_key
        let min_key = self.min_key.as_ref().map(|k| k.as_ref()).unwrap_or(&[]);
        buffer.extend_from_slice(&(min_key.len() as u32).to_le_bytes());
        buffer.extend_from_slice(min_key);

        // Write max_key
        let max_key = self.max_key.as_ref().map(|k| k.as_ref()).unwrap_or(&[]);
        buffer.extend_from_slice(&(max_key.len() as u32).to_le_bytes());
        buffer.extend_from_slice(max_key);

        // Single syscall instead of 4 syscalls
        self.file.write_all(&buffer)?;
        self.current_offset += buffer.len() as u64;

        Ok(())
    }

    fn write_footer(
        &mut self,
        top_level_offset: u64,
        bloom_offset: u64,
        metadata_offset: u64,
    ) -> Result<()> {
        let footer_start = self.current_offset;

        self.file.seek(SeekFrom::Start(0))?;
        let mut checksum = 0u32;
        let mut buf = vec![0u8; 4096];
        let mut remaining = footer_start;

        while remaining > 0 {
            let to_read = remaining.min(4096) as usize;
            self.file.read_exact(&mut buf[..to_read])?;
            checksum = crc32c::crc32c_append(checksum, &buf[..to_read]);
            remaining -= to_read as u64;
        }
        self.file.seek(SeekFrom::Start(footer_start))?;

        // OPTIMIZATION: Batch footer writes into single buffer (8 syscalls → 1 syscall)
        let mut footer_buffer = Vec::with_capacity(48); // Footer is exactly 48 bytes
        footer_buffer.extend_from_slice(&self.index_blocks_start.to_le_bytes());
        footer_buffer.extend_from_slice(&top_level_offset.to_le_bytes());
        footer_buffer.extend_from_slice(&bloom_offset.to_le_bytes());
        footer_buffer.extend_from_slice(&metadata_offset.to_le_bytes());
        footer_buffer.extend_from_slice(&checksum.to_le_bytes());
        footer_buffer.extend_from_slice(&MAGIC.to_le_bytes());
        footer_buffer.extend_from_slice(&VERSION.to_le_bytes());
        footer_buffer.extend_from_slice(&0u32.to_le_bytes());

        self.file.write_all(&footer_buffer)?;

        Ok(())
    }

    fn create_header() -> Vec<u8> {
        let mut header = Vec::with_capacity(32);
        header.extend_from_slice(&MAGIC.to_le_bytes()); // 4 bytes: magic
        header.extend_from_slice(&VERSION.to_le_bytes()); // 4 bytes: version
        header.extend_from_slice(&0u64.to_le_bytes()); // 8 bytes: reserved
        header.extend_from_slice(&0u64.to_le_bytes()); // 8 bytes: num_entries (filled in finish())
        header.extend_from_slice(&0u64.to_le_bytes()); // 8 bytes: max_sequence (filled in finish())
        header
    }
}

// ============================================================================
// BufferedSSTableBuilder - Build SSTable entirely in memory
// ============================================================================

/// SSTable builder that buffers all data in memory
/// Enables cloud storage uploads and reduces syscalls for local writes
pub struct BufferedSSTableBuilder {
    buffer: BytesMut,
    data_block: BlockBuilder,
    index_block: BlockBuilder,
    top_level_index: Vec<TopLevelIndexEntry>,
    bloom: BloomFilter,
    vlog_threshold: Option<usize>,
    num_entries: u64,
    current_offset: u64,
    index_blocks_start: u64,
    min_key: Option<Bytes>,
    max_key: Option<Bytes>,
    max_sequence: u64,
}

impl BufferedSSTableBuilder {
    /// Create a new buffered SSTable builder
    pub fn new() -> Self {
        let header = Self::create_header();
        let header_size = header.len() as u64;
        let mut buffer = BytesMut::with_capacity(64 * 1024); // Start with 64KB
        buffer.extend_from_slice(&header);

        Self {
            buffer,
            data_block: BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE),
            index_block: BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE),
            top_level_index: Vec::new(),
            bloom: BloomFilter::new(10000, 0.01),
            vlog_threshold: None,
            num_entries: 0,
            current_offset: header_size,
            index_blocks_start: 0,
            min_key: None,
            max_key: None,
            max_sequence: 0,
        }
    }

    pub fn with_vlog_threshold(mut self, threshold: usize) -> Self {
        self.vlog_threshold = Some(threshold);
        self
    }

    pub fn with_max_sequence(mut self, seq: u64) -> Self {
        self.max_sequence = seq;
        self
    }

    pub fn add(&mut self, key: Bytes, value: Bytes) -> Result<()> {
        if self.min_key.is_none() {
            self.min_key = Some(key.clone());
        }
        self.max_key = Some(key.clone());

        self.bloom.insert(&key);
        let entry = self.encode_entry(&key, FLAG_INLINE, &value);

        if !self.data_block.add(&key, &entry) {
            self.flush_data_block()?;
            if !self.data_block.add(&key, &entry) {
                let entry_size = key.len() + entry.len() + 8;
                let custom_size = (entry_size * 2).max(DEFAULT_BLOCK_SIZE * 2);
                self.data_block = BlockBuilder::with_capacity(custom_size);

                if !self.data_block.add(&key, &entry) {
                    return Err(SSTableError::InvalidFormat);
                }
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    pub fn add_raw(&mut self, key: Bytes, encoded_value: Bytes) -> Result<()> {
        if self.min_key.is_none() {
            self.min_key = Some(key.clone());
        }
        self.max_key = Some(key.clone());

        self.bloom.insert(&key);

        if !self.data_block.add(&key, &encoded_value) {
            self.flush_data_block()?;
            if !self.data_block.add(&key, &encoded_value) {
                let entry_size = key.len() + encoded_value.len() + 8;
                let custom_size = (entry_size * 2).max(DEFAULT_BLOCK_SIZE * 2);
                self.data_block = BlockBuilder::with_capacity(custom_size);

                if !self.data_block.add(&key, &encoded_value) {
                    return Err(SSTableError::InvalidFormat);
                }
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    pub fn add_with_vlog(&mut self, key: Bytes, value: Bytes, vlog: &mut VLog) -> Result<()> {
        self.bloom.insert(&key);

        // Use shared helper for vLog handling
        let (data, flag) = handle_vlog_value(&key, value, vlog, self.vlog_threshold)?;

        let entry = self.encode_entry(&key, flag, &data);

        if !self.data_block.add(&key, &entry) {
            self.flush_data_block()?;
            if !self.data_block.add(&key, &entry) {
                let entry_size = key.len() + entry.len() + 8;
                let custom_size = (entry_size * 2).max(DEFAULT_BLOCK_SIZE * 2);
                self.data_block = BlockBuilder::with_capacity(custom_size);

                if !self.data_block.add(&key, &entry) {
                    return Err(SSTableError::InvalidFormat);
                }
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    pub fn add_tombstone(&mut self, key: Bytes) -> Result<()> {
        self.bloom.insert(&key);
        let entry = self.encode_entry(&key, FLAG_TOMBSTONE, &[]);

        if !self.data_block.add(&key, &entry) {
            self.flush_data_block()?;
            if !self.data_block.add(&key, &entry) {
                let entry_size = key.len() + entry.len() + 8;
                let custom_size = (entry_size * 2).max(DEFAULT_BLOCK_SIZE * 2);
                self.data_block = BlockBuilder::with_capacity(custom_size);

                if !self.data_block.add(&key, &entry) {
                    return Err(SSTableError::InvalidFormat);
                }
            }
        }

        self.num_entries += 1;
        Ok(())
    }

    fn encode_entry(&self, _key: &[u8], flag: u8, data: &[u8]) -> Bytes {
        let mut buf = BytesMut::with_capacity(1 + data.len());
        buf.extend_from_slice(&[flag]);
        buf.extend_from_slice(data);
        buf.freeze()
    }

    fn flush_data_block(&mut self) -> Result<()> {
        if self.data_block.is_empty() {
            return Ok(());
        }

        let last_key = Bytes::copy_from_slice(self.data_block.last_key());
        let block_offset = self.current_offset;

        let old_block = std::mem::replace(
            &mut self.data_block,
            BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE),
        );
        let block_data = old_block.finish();
        let block_size = block_data.len() as u32;

        // Write to buffer instead of file
        self.buffer.extend_from_slice(&block_data);
        self.current_offset += block_data.len() as u64;

        let mut index_entry = BytesMut::with_capacity(4 + last_key.len() + 8 + 4);
        index_entry.extend_from_slice(&(last_key.len() as u32).to_le_bytes());
        index_entry.extend_from_slice(&last_key);
        index_entry.extend_from_slice(&block_offset.to_le_bytes());
        index_entry.extend_from_slice(&block_size.to_le_bytes());
        let index_entry_bytes = index_entry.freeze();

        if !self.index_block.add(&last_key, &index_entry_bytes) {
            self.flush_index_block()?;

            let mut index_entry2 = BytesMut::with_capacity(4 + last_key.len() + 8 + 4);
            index_entry2.extend_from_slice(&(last_key.len() as u32).to_le_bytes());
            index_entry2.extend_from_slice(&last_key);
            index_entry2.extend_from_slice(&block_offset.to_le_bytes());
            index_entry2.extend_from_slice(&block_size.to_le_bytes());

            if !self.index_block.add(&last_key, &index_entry2.freeze()) {
                return Err(SSTableError::InvalidFormat);
            }
        }

        Ok(())
    }

    fn flush_index_block(&mut self) -> Result<()> {
        if self.index_block.is_empty() {
            return Ok(());
        }

        if self.index_blocks_start == 0 {
            self.index_blocks_start = self.current_offset;
        }

        let last_key = Bytes::copy_from_slice(self.index_block.last_key());
        let block_offset = self.current_offset;

        let old_block = std::mem::replace(
            &mut self.index_block,
            BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE),
        );
        let block_data = old_block.finish();
        let block_size = block_data.len() as u32;

        // Write to buffer instead of file
        self.buffer.extend_from_slice(&block_data);
        self.current_offset += block_data.len() as u64;

        self.top_level_index.push(TopLevelIndexEntry {
            last_key,
            offset: block_offset,
            size: block_size,
        });

        Ok(())
    }

    /// Finish building and return the complete SSTable as bytes
    /// This is the primary method for cloud storage uploads
    pub fn finish_to_bytes(mut self) -> Result<Bytes> {
        self.flush_data_block()?;
        self.flush_index_block()?;

        let top_level_offset = self.current_offset;
        self.write_top_level_index();

        let bloom_offset = self.current_offset;
        let bloom_bytes = self.bloom.to_bytes();
        self.buffer
            .extend_from_slice(&(bloom_bytes.len() as u64).to_le_bytes());
        self.buffer.extend_from_slice(&bloom_bytes);
        self.current_offset += 8 + bloom_bytes.len() as u64;

        let metadata_offset = self.current_offset;
        self.write_metadata();

        // Update num_entries and max_sequence in header (offsets 16 and 24)
        let footer_offset = self.current_offset;
        self.buffer[16..24].copy_from_slice(&self.num_entries.to_le_bytes());
        self.buffer[24..32].copy_from_slice(&self.max_sequence.to_le_bytes());

        self.write_footer(top_level_offset, bloom_offset, metadata_offset, footer_offset);

        Ok(self.buffer.freeze())
    }

    /// Finish building and write to file (for local disk)
    pub fn finish_to_file(self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.finish_to_bytes()?;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        file.write_all(&bytes)?;
        file.sync_all()?;

        Ok(())
    }

    fn write_top_level_index(&mut self) {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&(self.top_level_index.len() as u32).to_le_bytes());

        for entry in &self.top_level_index {
            buffer.extend_from_slice(&(entry.last_key.len() as u32).to_le_bytes());
            buffer.extend_from_slice(&entry.last_key);
            buffer.extend_from_slice(&entry.offset.to_le_bytes());
            buffer.extend_from_slice(&entry.size.to_le_bytes());
        }

        self.buffer.extend_from_slice(&buffer);
        self.current_offset += buffer.len() as u64;
    }

    fn write_metadata(&mut self) {
        let mut buffer = Vec::new();

        let min_key = self.min_key.as_ref().map(|k| k.as_ref()).unwrap_or(&[]);
        buffer.extend_from_slice(&(min_key.len() as u32).to_le_bytes());
        buffer.extend_from_slice(min_key);

        let max_key = self.max_key.as_ref().map(|k| k.as_ref()).unwrap_or(&[]);
        buffer.extend_from_slice(&(max_key.len() as u32).to_le_bytes());
        buffer.extend_from_slice(max_key);

        self.buffer.extend_from_slice(&buffer);
        self.current_offset += buffer.len() as u64;
    }

    fn write_footer(
        &mut self,
        top_level_offset: u64,
        bloom_offset: u64,
        metadata_offset: u64,
        footer_offset: u64,
    ) {
        // Compute checksum over all data before footer
        let checksum = crc32c::crc32c(&self.buffer[..footer_offset as usize]);

        let mut footer_buffer = Vec::with_capacity(48);
        footer_buffer.extend_from_slice(&self.index_blocks_start.to_le_bytes());
        footer_buffer.extend_from_slice(&top_level_offset.to_le_bytes());
        footer_buffer.extend_from_slice(&bloom_offset.to_le_bytes());
        footer_buffer.extend_from_slice(&metadata_offset.to_le_bytes());
        footer_buffer.extend_from_slice(&checksum.to_le_bytes());
        footer_buffer.extend_from_slice(&MAGIC.to_le_bytes());
        footer_buffer.extend_from_slice(&VERSION.to_le_bytes());
        footer_buffer.extend_from_slice(&0u32.to_le_bytes());

        self.buffer.extend_from_slice(&footer_buffer);
    }

    fn create_header() -> Vec<u8> {
        let mut header = Vec::with_capacity(32);
        header.extend_from_slice(&MAGIC.to_le_bytes());
        header.extend_from_slice(&VERSION.to_le_bytes());
        header.extend_from_slice(&0u64.to_le_bytes()); // reserved
        header.extend_from_slice(&0u64.to_le_bytes()); // num_entries (filled later)
        header.extend_from_slice(&0u64.to_le_bytes()); // max_sequence (filled later)
        header
    }

    /// Returns true if no entries have been added
    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }

    /// Returns the number of entries added so far
    pub fn num_entries(&self) -> u64 {
        self.num_entries
    }
}

impl Default for BufferedSSTableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SSTable - Read SSTables with lazy loading
// ============================================================================

/// SSTable reader with lazy block loading
///
/// File handle optimization: Keeps file open for the lifetime of the SSTable to eliminate
/// repeated open/close overhead. Each SSTable maintains 1 file descriptor, bounded by the
/// number of SSTables in the LSM tree (typically 70-350 across all levels).
pub struct SSTable {
    file: Arc<Mutex<File>>, // File handle kept open for zero-overhead reads
    path: PathBuf,
    top_level_index: Vec<TopLevelIndexEntry>,
    #[allow(dead_code)] // Disabled due to key collision issues (Bug #9), binary search used instead
    alex_index: Option<AlexTree>, // ALEX learned index for faster lookups
    bloom: BloomFilter,
    num_entries: u64,
    vlog: Option<Arc<Mutex<VLog>>>,
    block_cache: Arc<Cache<u64, Block>>, // LRU cache with size limits
    min_key: Option<Bytes>,
    max_key: Option<Bytes>,
    /// Maximum sequence number in this SSTable
    /// Used to coordinate flush and compaction to prevent live key deletion
    max_sequence: u64,
    // Cache performance metrics (Arc for sharing with iterators)
    cache_hits: Arc<AtomicU64>,
    cache_misses: Arc<AtomicU64>,
    /// Optional global block cache shared across all SSTables
    /// Key: (path_hash, block_offset), Value: raw block data
    global_cache: Option<Arc<Cache<(u64, u64), Bytes>>>,
    /// Hash of this SSTable's path for global cache key
    path_hash: u64,
}

impl SSTable {
    /// Hash a path to a u64 for use as cache key
    fn hash_path(path: &Path) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_global_cache(path, None)
    }

    /// Open SSTable with optional global block cache
    ///
    /// When a global cache is provided, blocks are cached there with key (path_hash, offset).
    /// This allows hot blocks to be shared across all SSTables, improving cache hit rates.
    pub fn open_with_global_cache(
        path: impl AsRef<Path>,
        global_cache: Option<Arc<Cache<(u64, u64), Bytes>>>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let path_hash = Self::hash_path(&path);

        // Open file once and keep handle for the lifetime of this SSTable
        // This eliminates repeated open/close overhead (7.1x faster than opening per read)
        let mut file = File::open(&path)?;

        let (num_entries, max_sequence) = Self::read_header(&mut file)?;
        let (top_level_offset, bloom_offset, metadata_offset) = Self::read_footer(&mut file)?;
        let top_level_index = Self::load_top_level_index(&mut file, top_level_offset)?;
        let bloom = Self::load_bloom_filter(&mut file, bloom_offset)?;
        let (min_key, max_key) = Self::load_metadata(&mut file, metadata_offset)?;

        // Build ALEX learned index for faster top-level index lookups
        let alex_index = if !top_level_index.is_empty() {
            let mut alex = AlexTree::new();
            for (idx, entry) in top_level_index.iter().enumerate() {
                let key_i64 = bytes_to_i64(&entry.last_key);
                // Store index position as value (encoded as bytes)
                let value = (idx as u64).to_le_bytes().to_vec();
                if alex.insert(key_i64, value).is_err() {
                    // ALEX insert failed - fall back to binary search
                    break;
                }
            }
            Some(alex)
        } else {
            None
        };

        // Create LRU block cache with capacity for 10,000 blocks (~40MB at 4KB/block)
        // This is a local fallback cache if global cache is not provided
        let block_cache = Arc::new(Cache::new(10_000));

        Ok(Self {
            file: Arc::new(Mutex::new(file)), // Keep file handle for reuse
            path,
            top_level_index,
            alex_index,
            bloom,
            num_entries,
            vlog: None,
            block_cache,
            min_key,
            max_key,
            max_sequence,
            cache_hits: Arc::new(AtomicU64::new(0)),
            cache_misses: Arc::new(AtomicU64::new(0)),
            global_cache,
            path_hash,
        })
    }

    pub fn with_vlog(mut self, vlog: VLog) -> Self {
        self.vlog = Some(Arc::new(Mutex::new(vlog)));
        self
    }

    /// Get maximum sequence number in this SSTable
    pub fn max_sequence(&self) -> u64 {
        self.max_sequence
    }

    /// Get the minimum key in this SSTable (for range filtering)
    pub fn min_key(&self) -> Option<&Bytes> {
        self.min_key.as_ref()
    }

    /// Get the maximum key in this SSTable (for range filtering)
    pub fn max_key(&self) -> Option<&Bytes> {
        self.max_key.as_ref()
    }

    /// Check if this SSTable's key range overlaps with [start_key, end_key)
    pub fn overlaps_range(&self, start_key: &[u8], end_key: Option<&[u8]>) -> bool {
        // If we don't have metadata, assume it overlaps (conservative)
        let (min, max) = match (&self.min_key, &self.max_key) {
            (Some(min), Some(max)) => (min, max),
            _ => return true,
        };

        // Check if ranges overlap
        // Range [min, max] overlaps with [start_key, end_key) if:
        // max >= start_key AND (end_key is None OR min < end_key)
        if max.as_ref() < start_key {
            return false; // SSTable range is entirely before query range
        }

        if let Some(end) = end_key {
            if min.as_ref() >= end {
                return false; // SSTable range is entirely after query range
            }
        }

        true // Ranges overlap
    }

    /// Check if key might be in this SSTable (bloom filter check)
    pub fn may_contain(&self, key: &[u8]) -> bool {
        self.bloom.contains(key)
    }

    /// Get block cache statistics
    pub fn cache_stats(&self) -> (u64, u64, f64) {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };
        (hits, misses, hit_rate)
    }

    pub fn get(&mut self, key: &[u8]) -> Result<Option<Bytes>> {
        if !self.bloom.contains(key) {
            return Ok(None);
        }

        let (index_block_offset, index_block_size) = match self.find_index_block(key) {
            Some((offset, size)) => (offset, size),
            None => return Ok(None),
        };

        let index_block = self.load_block(index_block_offset, index_block_size)?;

        let (data_block_offset, data_block_size) =
            match self.find_in_index_block(&index_block, key)? {
                Some((offset, size)) => (offset, size),
                None => return Ok(None),
            };

        let data_block = self.load_block(data_block_offset, data_block_size)?;

        self.find_in_data_block(&data_block, key)
    }

    /// Check if a key exists in this SSTable (even if it's a tombstone)
    /// Returns true if the key is present (value or tombstone), false otherwise
    pub fn contains(&mut self, key: &[u8]) -> Result<bool> {
        if !self.bloom.contains(key) {
            return Ok(false);
        }

        let (index_block_offset, index_block_size) = match self.find_index_block(key) {
            Some((offset, size)) => (offset, size),
            None => return Ok(false),
        };

        let index_block = self.load_block(index_block_offset, index_block_size)?;

        let (data_block_offset, data_block_size) =
            match self.find_in_index_block(&index_block, key)? {
                Some((offset, size)) => (offset, size),
                None => return Ok(false),
            };

        let data_block = self.load_block(data_block_offset, data_block_size)?;

        // Check if key exists in data block
        Ok(data_block.find_exact(key).is_some())
    }

    fn find_index_block(&self, key: &[u8]) -> Option<(u64, u32)> {
        // CRITICAL FIX (Bug #11): Disable ALEX for top-level index lookup
        // ALEX learned index cannot correctly handle keys with shared prefixes
        // (e.g., "key_0000000000" and "key_0000000100" produce non-monotonic i64 values)
        // The partition_point binary search is correct and fast (O(log N) where N is typically 2-10)
        let idx = self
            .top_level_index
            .partition_point(|entry| entry.last_key.as_ref() < key);

        if idx < self.top_level_index.len() {
            Some((
                self.top_level_index[idx].offset,
                self.top_level_index[idx].size,
            ))
        } else {
            self.top_level_index.last().map(|e| (e.offset, e.size))
        }
    }

    fn find_in_index_block(&self, index_block: &Block, key: &[u8]) -> Result<Option<(u64, u32)>> {
        // Binary search for first entry where entry_key >= key
        let entry = match index_block.find_lower_bound(key) {
            Some(entry) => entry,
            None => return Ok(None),
        };

        let (_entry_key, entry_value) = entry;
        let value_len = entry_value.len();

        if value_len < 12 {
            return Err(SSTableError::InvalidFormat);
        }

        let mut offset_bytes = [0u8; 8];
        let mut size_bytes = [0u8; 4];
        offset_bytes.copy_from_slice(&entry_value[value_len - 12..value_len - 4]);
        size_bytes.copy_from_slice(&entry_value[value_len - 4..]);

        let offset = u64::from_le_bytes(offset_bytes);
        let size = u32::from_le_bytes(size_bytes);

        Ok(Some((offset, size)))
    }

    fn find_in_data_block(&mut self, data_block: &Block, key: &[u8]) -> Result<Option<Bytes>> {
        // Binary search for exact key match
        let (_entry_key, entry_value) = match data_block.find_exact(key) {
            Some(entry) => entry,
            None => return Ok(None),
        };

        if entry_value.is_empty() {
            return Err(SSTableError::InvalidFormat);
        }

        let flag = entry_value[0];
        let data = entry_value.slice(1..);

        match flag {
            FLAG_INLINE => Ok(Some(data)),
            FLAG_POINTER => {
                if data.len() < 12 {
                    return Err(SSTableError::InvalidFormat);
                }

                let offset = u64::from_le_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]);
                let length = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

                if let Some(ref vlog) = self.vlog {
                    let mut vlog_guard = vlog.lock().unwrap();
                    let pointer = ValuePointer { offset, length };
                    let value = vlog_guard
                        .read(pointer)
                        .map_err(|e| SSTableError::VLog(e.to_string()))?;
                    Ok(Some(value))
                } else {
                    Err(SSTableError::VLog("VLog not attached".to_string()))
                }
            }
            FLAG_TOMBSTONE => Ok(None),
            _ => Err(SSTableError::InvalidFormat),
        }
    }

    fn load_block(&self, offset: u64, size: u32) -> Result<Block> {
        // Fast path 1: Check global cache first (shared across all SSTables)
        if let Some(ref global) = self.global_cache {
            let cache_key = (self.path_hash, offset);
            if let Some(block_data) = global.get(&cache_key) {
                // Global cache hit!
                self.cache_hits.fetch_add(1, Ordering::Relaxed);
                // Parse the cached bytes into a Block (no CRC check needed - already verified)
                return Block::new(block_data)
                    .map_err(|e| SSTableError::Io(std::io::Error::other(e)));
            }
        }

        // Fast path 2: Check local cache (per-SSTable fallback)
        if let Some(block) = self.block_cache.get(&offset) {
            // Local cache hit!
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(block);
        }

        // Cache miss - record and load from disk
        self.cache_misses.fetch_add(1, Ordering::Relaxed);

        // Slow path: Reuse file handle for zero-overhead reads
        // File was opened in SSTable::open() and kept alive for our lifetime
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;

        let mut buf = vec![0u8; size as usize];
        file.read_exact(&mut buf)?;
        let block_data = Bytes::from(buf);
        drop(file); // Release lock before CRC verification

        // Parse and verify block (CRC check happens here)
        let block = Block::new(block_data.clone())?;

        // Cache the verified block in global cache (if available)
        if let Some(ref global) = self.global_cache {
            let cache_key = (self.path_hash, offset);
            global.insert(cache_key, block_data);
        }

        // Also cache in local cache (automatic LRU eviction when full)
        self.block_cache.insert(offset, block.clone());

        Ok(block)
    }

    fn read_header(file: &mut File) -> Result<(u64, u64)> {
        file.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; 32];
        file.read_exact(&mut header)?;

        let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

        if magic != MAGIC || version != VERSION {
            return Err(SSTableError::InvalidFormat);
        }

        let num_entries = u64::from_le_bytes([
            header[16], header[17], header[18], header[19], header[20], header[21], header[22],
            header[23],
        ]);

        let max_sequence = u64::from_le_bytes([
            header[24], header[25], header[26], header[27], header[28], header[29], header[30],
            header[31],
        ]);

        Ok((num_entries, max_sequence))
    }

    fn read_footer(file: &mut File) -> Result<(u64, u64, u64)> {
        let file_size = file.metadata()?.len();
        file.seek(SeekFrom::Start(file_size - 48))?; // v4: 48 bytes (added metadata_offset)

        let mut footer = [0u8; 48];
        file.read_exact(&mut footer)?;

        let _index_blocks_offset = u64::from_le_bytes([
            footer[0], footer[1], footer[2], footer[3], footer[4], footer[5], footer[6], footer[7],
        ]);
        let top_level_offset = u64::from_le_bytes([
            footer[8], footer[9], footer[10], footer[11], footer[12], footer[13], footer[14],
            footer[15],
        ]);
        let bloom_offset = u64::from_le_bytes([
            footer[16], footer[17], footer[18], footer[19], footer[20], footer[21], footer[22],
            footer[23],
        ]);
        let metadata_offset = u64::from_le_bytes([
            footer[24], footer[25], footer[26], footer[27], footer[28], footer[29], footer[30],
            footer[31],
        ]);

        let stored_checksum = u32::from_le_bytes([footer[32], footer[33], footer[34], footer[35]]);
        let magic = u32::from_le_bytes([footer[36], footer[37], footer[38], footer[39]]);
        let version = u32::from_le_bytes([footer[40], footer[41], footer[42], footer[43]]);

        if magic != MAGIC || version != VERSION {
            return Err(SSTableError::InvalidFormat);
        }

        // Validate checksum over entire file content (before footer)
        let footer_start = file_size - 48;
        file.seek(SeekFrom::Start(0))?;
        let mut computed_checksum = 0u32;
        let mut buf = vec![0u8; 4096];
        let mut remaining = footer_start;

        while remaining > 0 {
            let to_read = remaining.min(4096) as usize;
            file.read_exact(&mut buf[..to_read])?;
            computed_checksum = crc32c::crc32c_append(computed_checksum, &buf[..to_read]);
            remaining -= to_read as u64;
        }

        if computed_checksum != stored_checksum {
            return Err(SSTableError::Corruption {
                expected: stored_checksum,
                actual: computed_checksum,
            });
        }

        Ok((top_level_offset, bloom_offset, metadata_offset))
    }

    fn load_metadata(file: &mut File, offset: u64) -> Result<(Option<Bytes>, Option<Bytes>)> {
        file.seek(SeekFrom::Start(offset))?;

        // Read min_key
        let mut len_buf = [0u8; 4];
        file.read_exact(&mut len_buf)?;
        let min_key_len = u32::from_le_bytes(len_buf) as usize;
        let min_key = if min_key_len > 0 {
            let mut key_buf = vec![0u8; min_key_len];
            file.read_exact(&mut key_buf)?;
            Some(Bytes::from(key_buf))
        } else {
            None
        };

        // Read max_key
        file.read_exact(&mut len_buf)?;
        let max_key_len = u32::from_le_bytes(len_buf) as usize;
        let max_key = if max_key_len > 0 {
            let mut key_buf = vec![0u8; max_key_len];
            file.read_exact(&mut key_buf)?;
            Some(Bytes::from(key_buf))
        } else {
            None
        };

        Ok((min_key, max_key))
    }

    fn load_top_level_index(file: &mut File, offset: u64) -> Result<Vec<TopLevelIndexEntry>> {
        file.seek(SeekFrom::Start(offset))?;

        let mut num_entries_buf = [0u8; 4];
        file.read_exact(&mut num_entries_buf)?;
        let num_entries = u32::from_le_bytes(num_entries_buf) as usize;

        let mut entries = Vec::with_capacity(num_entries);

        for _ in 0..num_entries {
            let mut key_len_buf = [0u8; 4];
            file.read_exact(&mut key_len_buf)?;
            let key_len = u32::from_le_bytes(key_len_buf) as usize;

            let mut key = vec![0u8; key_len];
            file.read_exact(&mut key)?;

            let mut offset_buf = [0u8; 8];
            file.read_exact(&mut offset_buf)?;
            let block_offset = u64::from_le_bytes(offset_buf);

            let mut size_buf = [0u8; 4];
            file.read_exact(&mut size_buf)?;
            let block_size = u32::from_le_bytes(size_buf);

            entries.push(TopLevelIndexEntry {
                last_key: Bytes::from(key),
                offset: block_offset,
                size: block_size,
            });
        }

        Ok(entries)
    }

    fn load_bloom_filter(file: &mut File, offset: u64) -> Result<BloomFilter> {
        file.seek(SeekFrom::Start(offset))?;

        let mut len_buf = [0u8; 8];
        file.read_exact(&mut len_buf)?;
        let bloom_len = u64::from_le_bytes(len_buf) as usize;

        let mut bloom_bytes = vec![0u8; bloom_len];
        file.read_exact(&mut bloom_bytes)?;

        BloomFilter::from_bytes(&bloom_bytes).ok_or(SSTableError::InvalidFormat)
    }

    pub fn len(&self) -> usize {
        self.num_entries as usize
    }

    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Validate all blocks in the SSTable by loading and checking checksums
    /// This is expensive but useful for corruption detection
    pub fn validate(&mut self) -> Result<()> {
        let file_size = std::fs::metadata(&self.path)?.len();

        for top_entry in &self.top_level_index {
            if top_entry.offset + (top_entry.size as u64) > file_size {
                return Err(SSTableError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "Block extends past end of file: offset={}, size={}, file_size={}",
                        top_entry.offset, top_entry.size, file_size
                    ),
                )));
            }

            let index_block = self.load_block(top_entry.offset, top_entry.size)?;

            for result in index_block.iter() {
                let (_key_bytes, value_bytes) = result?;

                // Index entry format: [key_len: 4][key: variable][offset: 8][size: 4]
                if value_bytes.len() < 16 {
                    continue;
                }

                let key_len = u32::from_le_bytes(value_bytes[..4].try_into().unwrap()) as usize;
                let offset_start = 4 + key_len;

                if value_bytes.len() < offset_start + 12 {
                    continue;
                }

                let offset = u64::from_le_bytes(
                    value_bytes[offset_start..offset_start + 8]
                        .try_into()
                        .unwrap(),
                );
                let size = u32::from_le_bytes(
                    value_bytes[offset_start + 8..offset_start + 12]
                        .try_into()
                        .unwrap(),
                );

                if offset + (size as u64) > file_size {
                    return Err(SSTableError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "Data block extends past end of file: offset={}, size={}, file_size={}",
                            offset, size, file_size
                        ),
                    )));
                }

                let _data_block = self.load_block(offset, size)?;
            }
        }

        Ok(())
    }
}

// ============================================================================
// SSTableIterator - Iterator over all entries
// ============================================================================

pub struct SSTableIterator {
    entries: std::vec::IntoIter<(Bytes, Bytes)>,
}

// ============================================================================
// SSTableRangeIterator - Lazy iterator over a key range
// ============================================================================

/// Lazy iterator that loads blocks on-demand during range scans
pub struct SSTableRangeIterator {
    file: Arc<Mutex<File>>, // Reuse file handle from parent SSTable
    block_cache: Arc<Cache<u64, Block>>,
    vlog: Option<Arc<Mutex<VLog>>>,
    top_level_index: Vec<TopLevelIndexEntry>,
    start_key: Bytes,
    end_key: Option<Bytes>,
    // Cache performance metrics (shared with parent SSTable)
    cache_hits: Arc<AtomicU64>,
    cache_misses: Arc<AtomicU64>,

    // Iteration state
    top_idx: usize,
    current_index_block: Option<Block>,
    index_block_entries: Vec<(u64, u32)>, // (offset, size) pairs for data blocks
    index_entry_idx: usize,
    // Store entries from current data block (avoids lifetime issues with iterator)
    current_block_entries: Vec<(Bytes, Bytes)>,
    current_entry_idx: usize,
}

impl SSTableRangeIterator {
    fn new(
        file: Arc<Mutex<File>>, // Reuse file handle from parent SSTable
        block_cache: Arc<Cache<u64, Block>>,
        vlog: Option<Arc<Mutex<VLog>>>,
        top_level_index: Vec<TopLevelIndexEntry>,
        start_key: &[u8],
        end_key: Option<&[u8]>,
        cache_hits: Arc<AtomicU64>,
        cache_misses: Arc<AtomicU64>,
    ) -> Self {
        Self {
            file,
            block_cache,
            vlog,
            top_level_index,
            start_key: Bytes::copy_from_slice(start_key),
            end_key: end_key.map(|k| Bytes::copy_from_slice(k)),
            cache_hits,
            cache_misses,
            top_idx: 0,
            current_index_block: None,
            index_block_entries: Vec::new(),
            index_entry_idx: 0,
            current_block_entries: Vec::new(),
            current_entry_idx: 0,
        }
    }

    fn load_block(&self, offset: u64, size: u32) -> Result<Block> {
        // Check cache first (quick_cache is lock-free!)
        if let Some(block) = self.block_cache.get(&offset) {
            // Cache hit!
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(block);
        }

        // Cache miss - record and load from disk
        self.cache_misses.fetch_add(1, Ordering::Relaxed);

        // Reuse file handle from parent SSTable
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;

        let mut buf = vec![0u8; size as usize];
        file.read_exact(&mut buf)?;
        let block_data = Bytes::from(buf);
        drop(file); // Release lock

        // Parse and verify block
        let block = Block::new(block_data)?;

        // Cache the block (automatic LRU eviction when full)
        self.block_cache.insert(offset, block.clone());

        Ok(block)
    }

    fn advance_to_next_index_block(&mut self) -> Result<bool> {
        // Find next relevant top-level index block
        while self.top_idx < self.top_level_index.len() {
            let top_entry = &self.top_level_index[self.top_idx];

            // Skip blocks that end before our start key
            if top_entry.last_key.as_ref() < self.start_key.as_ref() {
                self.top_idx += 1;
                continue;
            }

            // Stop if we've gone past end_key
            if let Some(ref end) = self.end_key {
                if top_entry.last_key.as_ref() >= end.as_ref() && self.current_index_block.is_some()
                {
                    return Ok(false);
                }
            }

            // Load this index block
            let index_block = self.load_block(top_entry.offset, top_entry.size)?;

            // Extract data block offsets/sizes from index block
            self.index_block_entries.clear();
            for entry_result in index_block.iter() {
                let (_key, value) = entry_result?;

                if value.len() < 12 {
                    continue;
                }

                let value_len = value.len();
                let mut offset_bytes = [0u8; 8];
                let mut size_bytes = [0u8; 4];
                offset_bytes.copy_from_slice(&value[value_len - 12..value_len - 4]);
                size_bytes.copy_from_slice(&value[value_len - 4..]);

                let offset = u64::from_le_bytes(offset_bytes);
                let size = u32::from_le_bytes(size_bytes);

                self.index_block_entries.push((offset, size));
            }

            self.current_index_block = Some(index_block);
            self.index_entry_idx = 0;
            self.top_idx += 1;

            return Ok(true);
        }

        Ok(false)
    }

    fn advance_to_next_data_block(&mut self) -> Result<bool> {
        if self.index_entry_idx >= self.index_block_entries.len() {
            // Need next index block
            if !self.advance_to_next_index_block()? {
                return Ok(false);
            }
        }

        if self.index_entry_idx < self.index_block_entries.len() {
            let (offset, size) = self.index_block_entries[self.index_entry_idx];
            let data_block = self.load_block(offset, size)?;

            // Extract entries from the block (avoids iterator lifetime issues)
            self.current_block_entries.clear();
            for entry_result in data_block.iter() {
                let (key, value) = entry_result?;
                self.current_block_entries.push((key, value));
            }

            self.current_entry_idx = 0;
            self.index_entry_idx += 1;

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Iterator for SSTableRangeIterator {
    type Item = Result<(Bytes, Option<Bytes>)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Try to get next entry from current data block
            while self.current_entry_idx < self.current_block_entries.len() {
                let (key, entry_value) = &self.current_block_entries[self.current_entry_idx];
                self.current_entry_idx += 1;

                // Check if key is in range
                if key.as_ref() < self.start_key.as_ref() {
                    continue;
                }
                if let Some(ref end) = self.end_key {
                    if key.as_ref() >= end.as_ref() {
                        return None; // Past end of range
                    }
                }

                // Decode value
                if entry_value.is_empty() {
                    continue;
                }

                let flag = entry_value[0];
                let data = entry_value.slice(1..);

                let value_opt = match flag {
                    FLAG_INLINE => Some(data),
                    FLAG_POINTER => {
                        if data.len() < 12 {
                            continue;
                        }

                        let offset = u64::from_le_bytes([
                            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                        ]);
                        let length = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

                        if let Some(ref vlog) = self.vlog {
                            let mut vlog_guard = vlog.lock().unwrap();
                            let pointer = ValuePointer { offset, length };
                            match vlog_guard.read(pointer) {
                                Ok(value) => Some(value),
                                Err(e) => return Some(Err(SSTableError::VLog(e.to_string()))),
                            }
                        } else {
                            return Some(Err(SSTableError::VLog("VLog not attached".to_string())));
                        }
                    }
                    FLAG_TOMBSTONE => None,
                    _ => continue,
                };

                return Some(Ok((key.clone(), value_opt)));
            }

            // Need to advance to next data block
            match self.advance_to_next_data_block() {
                Ok(true) => continue,
                Ok(false) => return None,
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

impl SSTable {
    pub fn iter(&mut self) -> Result<SSTableIterator> {
        let mut entries = Vec::new();

        for top_entry in &self.top_level_index {
            let index_block = self.load_block(top_entry.offset, top_entry.size)?;

            for idx_entry in index_block.iter() {
                let (_index_key, index_value) = idx_entry?;

                let value_len = index_value.len();
                if value_len < 12 {
                    continue;
                }

                let mut offset_bytes = [0u8; 8];
                let mut size_bytes = [0u8; 4];
                offset_bytes.copy_from_slice(&index_value[value_len - 12..value_len - 4]);
                size_bytes.copy_from_slice(&index_value[value_len - 4..]);

                let data_offset = u64::from_le_bytes(offset_bytes);
                let data_size = u32::from_le_bytes(size_bytes);

                let data_block = self.load_block(data_offset, data_size)?;

                for data_entry in data_block.iter() {
                    let (key, entry_value) = data_entry?;

                    if entry_value.is_empty() {
                        continue;
                    }

                    let flag = entry_value[0];
                    let data = entry_value.slice(1..);

                    let value = match flag {
                        FLAG_INLINE => {
                            if self.vlog.is_some() {
                                // vlog attached - return decoded value
                                data
                            } else {
                                // No vlog attached (compaction) - return full entry with FLAG
                                entry_value
                            }
                        }
                        FLAG_POINTER => {
                            if data.len() < 12 {
                                continue;
                            }

                            let offset = u64::from_le_bytes([
                                data[0], data[1], data[2], data[3], data[4], data[5], data[6],
                                data[7],
                            ]);
                            let length = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

                            if let Some(ref vlog) = self.vlog {
                                let mut vlog_guard = vlog.lock().unwrap();
                                let pointer = ValuePointer { offset, length };
                                vlog_guard
                                    .read(pointer)
                                    .map_err(|e| SSTableError::VLog(e.to_string()))?
                            } else {
                                // No vlog attached (e.g., during compaction)
                                // Return the full entry including FLAG_POINTER + pointer bytes
                                // so compaction can preserve vlog pointers
                                entry_value
                            }
                        }
                        FLAG_TOMBSTONE => {
                            // CRITICAL FIX (Bug #7): Preserve tombstones during compaction
                            // Tombstones MUST be copied to output SSTables to prevent deleted keys
                            // from resurrecting from older levels after compaction completes.
                            if self.vlog.is_some() {
                                // User-facing read: filter out tombstones (deleted keys)
                                continue;
                            } else {
                                // Compaction: preserve tombstones in output SSTable
                                entry_value
                            }
                        }
                        _ => continue,
                    };

                    entries.push((key, value));
                }
            }
        }

        Ok(SSTableIterator {
            entries: entries.into_iter(),
        })
    }

    /// Scan a range of keys from this SSTable using lazy iteration
    ///
    /// Returns an iterator that yields (key, Option<value>) where None indicates a tombstone.
    /// Blocks are loaded on-demand as the iterator is consumed, avoiding upfront materialization.
    pub fn scan_range(&self, start_key: &[u8], end_key: Option<&[u8]>) -> SSTableRangeIterator {
        SSTableRangeIterator::new(
            Arc::clone(&self.file), // Share file handle with iterator
            Arc::clone(&self.block_cache),
            self.vlog.as_ref().map(Arc::clone),
            self.top_level_index.clone(),
            start_key,
            end_key,
            Arc::clone(&self.cache_hits),
            Arc::clone(&self.cache_misses),
        )
    }
}

impl Iterator for SSTableIterator {
    type Item = Result<(Bytes, Bytes)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(Ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_buffered_builder_basic() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        // Build SSTable with buffered builder
        let mut builder = BufferedSSTableBuilder::new();
        builder
            .add(Bytes::from("key1"), Bytes::from("value1"))
            .unwrap();
        builder
            .add(Bytes::from("key2"), Bytes::from("value2"))
            .unwrap();
        builder
            .add(Bytes::from("key3"), Bytes::from("value3"))
            .unwrap();

        // Finish to file
        builder.finish_to_file(&path).unwrap();

        // Read back with SSTable
        let mut sst = SSTable::open(&path).unwrap();
        assert_eq!(sst.num_entries, 3);
        assert_eq!(
            sst.get(b"key1").unwrap().unwrap(),
            Bytes::from("value1")
        );
        assert_eq!(
            sst.get(b"key2").unwrap().unwrap(),
            Bytes::from("value2")
        );
        assert_eq!(
            sst.get(b"key3").unwrap().unwrap(),
            Bytes::from("value3")
        );
    }

    #[test]
    fn test_buffered_builder_to_bytes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        // Build SSTable and get bytes
        let mut builder = BufferedSSTableBuilder::new();
        builder
            .add(Bytes::from("aaa"), Bytes::from("111"))
            .unwrap();
        builder
            .add(Bytes::from("bbb"), Bytes::from("222"))
            .unwrap();

        let bytes = builder.finish_to_bytes().unwrap();

        // Write bytes to file manually
        std::fs::write(&path, &bytes).unwrap();

        // Read back
        let mut sst = SSTable::open(&path).unwrap();
        assert_eq!(sst.num_entries, 2);
        assert_eq!(sst.get(b"aaa").unwrap().unwrap(), Bytes::from("111"));
        assert_eq!(sst.get(b"bbb").unwrap().unwrap(), Bytes::from("222"));
    }

    #[test]
    fn test_buffered_builder_empty() {
        let builder = BufferedSSTableBuilder::new();
        assert!(builder.is_empty());
        assert_eq!(builder.num_entries(), 0);

        // Even empty builder should produce valid SSTable bytes
        let bytes = builder.finish_to_bytes().unwrap();
        assert!(bytes.len() > 0);

        // Should contain header (32 bytes) + footer (48 bytes) minimum
        assert!(bytes.len() >= 80);
    }

    #[test]
    fn test_buffered_builder_tombstone() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        let mut builder = BufferedSSTableBuilder::new();
        builder
            .add(Bytes::from("key1"), Bytes::from("value1"))
            .unwrap();
        builder.add_tombstone(Bytes::from("key2")).unwrap();
        builder
            .add(Bytes::from("key3"), Bytes::from("value3"))
            .unwrap();

        builder.finish_to_file(&path).unwrap();

        let mut sst = SSTable::open(&path).unwrap();
        assert_eq!(sst.num_entries, 3);
        assert_eq!(
            sst.get(b"key1").unwrap().unwrap(),
            Bytes::from("value1")
        );
        // key2 is a tombstone - should return None
        assert!(sst.get(b"key2").unwrap().is_none());
        assert_eq!(
            sst.get(b"key3").unwrap().unwrap(),
            Bytes::from("value3")
        );
    }

    #[test]
    fn test_buffered_builder_max_sequence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        let mut builder = BufferedSSTableBuilder::new().with_max_sequence(12345);
        builder
            .add(Bytes::from("key1"), Bytes::from("value1"))
            .unwrap();

        builder.finish_to_file(&path).unwrap();

        let sst = SSTable::open(&path).unwrap();
        assert_eq!(sst.max_sequence(), 12345);
    }

    #[test]
    fn test_buffered_builder_many_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        let mut builder = BufferedSSTableBuilder::new();

        // Add 1000 entries to force multiple data blocks
        for i in 0..1000 {
            let key = format!("key{:06}", i);
            let value = format!("value{:06}", i);
            builder
                .add(Bytes::from(key), Bytes::from(value))
                .unwrap();
        }

        assert_eq!(builder.num_entries(), 1000);

        builder.finish_to_file(&path).unwrap();

        let mut sst = SSTable::open(&path).unwrap();
        assert_eq!(sst.num_entries, 1000);

        // Spot check
        assert_eq!(
            sst.get(b"key000000").unwrap().unwrap(),
            Bytes::from("value000000")
        );
        assert_eq!(
            sst.get(b"key000500").unwrap().unwrap(),
            Bytes::from("value000500")
        );
        assert_eq!(
            sst.get(b"key000999").unwrap().unwrap(),
            Bytes::from("value000999")
        );
    }

    #[test]
    fn test_buffered_builder_checksum_valid() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        let mut builder = BufferedSSTableBuilder::new();
        builder
            .add(Bytes::from("key1"), Bytes::from("value1"))
            .unwrap();

        let bytes = builder.finish_to_bytes().unwrap();
        std::fs::write(&path, &bytes).unwrap();

        // Opening should validate checksum
        let sst = SSTable::open(&path).unwrap();
        assert_eq!(sst.num_entries, 1);
    }

    #[test]
    fn test_buffered_vs_file_builder_equivalence() {
        let dir = tempdir().unwrap();
        let path_buffered = dir.path().join("buffered.sst");
        let path_file = dir.path().join("file.sst");

        // Build with buffered builder
        let mut buffered = BufferedSSTableBuilder::new().with_max_sequence(100);
        buffered
            .add(Bytes::from("key1"), Bytes::from("value1"))
            .unwrap();
        buffered
            .add(Bytes::from("key2"), Bytes::from("value2"))
            .unwrap();
        buffered.finish_to_file(&path_buffered).unwrap();

        // Build with file-based builder
        let mut file_builder =
            SSTableBuilder::create(&path_file).unwrap().with_max_sequence(100);
        file_builder
            .add(Bytes::from("key1"), Bytes::from("value1"))
            .unwrap();
        file_builder
            .add(Bytes::from("key2"), Bytes::from("value2"))
            .unwrap();
        file_builder.finish().unwrap();

        // Both should be readable and contain same data
        let mut sst_buffered = SSTable::open(&path_buffered).unwrap();
        let mut sst_file = SSTable::open(&path_file).unwrap();

        assert_eq!(sst_buffered.num_entries, sst_file.num_entries);
        assert_eq!(sst_buffered.max_sequence(), sst_file.max_sequence());
        assert_eq!(
            sst_buffered.get(b"key1").unwrap(),
            sst_file.get(b"key1").unwrap()
        );
        assert_eq!(
            sst_buffered.get(b"key2").unwrap(),
            sst_file.get(b"key2").unwrap()
        );
    }

    #[test]
    fn test_buffered_builder_large_value() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        let mut builder = BufferedSSTableBuilder::new();
        let large_value = vec![0xABu8; 100_000]; // 100KB value

        builder
            .add(Bytes::from("large"), Bytes::from(large_value.clone()))
            .unwrap();

        builder.finish_to_file(&path).unwrap();

        let mut sst = SSTable::open(&path).unwrap();
        assert_eq!(sst.get(b"large").unwrap().unwrap().as_ref(), &large_value);
    }
}

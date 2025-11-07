// SSTable: Sorted String Table on disk
// Block-based format with lazy loading for memory efficiency

pub mod block;

use crate::alex::AlexTree;
use crate::bloom::BloomFilter;
use block::{BlockBuilder, BlockError, Block, DEFAULT_BLOCK_SIZE};
use crate::vlog::{VLog, ValuePointer};
use bytes::{Bytes, BytesMut};
use crc32fast::Hasher;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
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
const VERSION: u32 = 0x00000003; // v3: Bit-packed bloom filter (Vec<u64> storage)

/// Entry value type flags
pub const FLAG_INLINE: u8 = 0x00;
pub const FLAG_POINTER: u8 = 0x01;
pub const FLAG_TOMBSTONE: u8 = 0x02;

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
    let mut buf = [0u8; 8];
    let len = bytes.len().min(8);
    buf[..len].copy_from_slice(&bytes[..len]);
    i64::from_be_bytes(buf)
}

// ============================================================================
// SSTableBuilder - Write SSTables incrementally
// ============================================================================

/// SSTable builder with block-based format
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
        })
    }

    pub fn with_vlog_threshold(mut self, threshold: usize) -> Self {
        self.vlog_threshold = Some(threshold);
        self
    }

    pub fn add(&mut self, key: Bytes, value: Bytes) -> Result<()> {
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

        let (flag, data) = if let Some(threshold) = self.vlog_threshold {
            if value.len() > threshold {
                let pointer = vlog
                    .append(&key, &value)
                    .map_err(|e| SSTableError::VLog(e.to_string()))?;

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

        let old_block = std::mem::replace(&mut self.data_block, BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE));
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

        let old_block = std::mem::replace(&mut self.index_block, BlockBuilder::with_capacity(DEFAULT_BLOCK_SIZE));
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
        self.file.write_all(&(bloom_bytes.len() as u64).to_le_bytes())?;
        self.file.write_all(&bloom_bytes)?;
        self.current_offset += 8 + bloom_bytes.len() as u64;

        self.write_footer(top_level_offset, bloom_offset)?;

        self.file.seek(SeekFrom::Start(8))?;
        self.file.write_all(&0u64.to_le_bytes())?;
        self.file.write_all(&self.num_entries.to_le_bytes())?;

        self.file.sync_all()?;
        Ok(())
    }

    fn write_top_level_index(&mut self) -> Result<()> {
        self.file.write_all(&(self.top_level_index.len() as u32).to_le_bytes())?;
        self.current_offset += 4;

        for entry in &self.top_level_index {
            self.file.write_all(&(entry.last_key.len() as u32).to_le_bytes())?;
            self.file.write_all(&entry.last_key)?;
            self.file.write_all(&entry.offset.to_le_bytes())?;
            self.file.write_all(&entry.size.to_le_bytes())?;
            self.current_offset += 4 + entry.last_key.len() as u64 + 8 + 4;
        }

        Ok(())
    }

    fn write_footer(&mut self, top_level_offset: u64, bloom_offset: u64) -> Result<()> {
        let footer_start = self.current_offset;

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
        self.file.seek(SeekFrom::Start(footer_start))?;

        self.file.write_all(&self.index_blocks_start.to_le_bytes())?;
        self.file.write_all(&top_level_offset.to_le_bytes())?;
        self.file.write_all(&bloom_offset.to_le_bytes())?;
        self.file.write_all(&checksum.to_le_bytes())?;
        self.file.write_all(&MAGIC.to_le_bytes())?;
        self.file.write_all(&VERSION.to_le_bytes())?;
        self.file.write_all(&0u32.to_le_bytes())?;

        Ok(())
    }

    fn create_header() -> Vec<u8> {
        let mut header = Vec::with_capacity(32);
        header.extend_from_slice(&MAGIC.to_le_bytes());
        header.extend_from_slice(&VERSION.to_le_bytes());
        header.extend_from_slice(&0u64.to_le_bytes());
        header.extend_from_slice(&0u64.to_le_bytes());
        header.extend_from_slice(&0u64.to_le_bytes());
        header
    }
}

// ============================================================================
// SSTable - Read SSTables with lazy loading
// ============================================================================

/// SSTable reader with lazy block loading
pub struct SSTable {
    file: Arc<Mutex<File>>,
    path: PathBuf,
    top_level_index: Vec<TopLevelIndexEntry>,
    alex_index: Option<AlexTree>, // ALEX learned index for faster lookups
    bloom: BloomFilter,
    num_entries: u64,
    vlog: Option<Arc<Mutex<VLog>>>,
    block_cache: Arc<Mutex<HashMap<u64, Bytes>>>,
}

impl SSTable {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;

        let (num_entries, _) = Self::read_header(&mut file)?;
        let (top_level_offset, bloom_offset) = Self::read_footer(&mut file)?;
        let top_level_index = Self::load_top_level_index(&mut file, top_level_offset)?;
        let bloom = Self::load_bloom_filter(&mut file, bloom_offset)?;

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

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            path,
            top_level_index,
            alex_index,
            bloom,
            num_entries,
            vlog: None,
            block_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn with_vlog(mut self, vlog: VLog) -> Self {
        self.vlog = Some(Arc::new(Mutex::new(vlog)));
        self
    }

    /// Check if key might be in this SSTable (bloom filter check)
    pub fn may_contain(&self, key: &[u8]) -> bool {
        self.bloom.contains(key)
    }

    pub fn get(&mut self, key: &[u8]) -> Result<Option<Bytes>> {
        if !self.bloom.contains(key) {
            return Ok(None);
        }

        let (index_block_offset, index_block_size) = match self.find_index_block(key) {
            Some((offset, size)) => (offset, size),
            None => return Ok(None),
        };

        let index_block_data = self.load_block(index_block_offset, index_block_size)?;
        let index_block = Block::new(index_block_data)?;

        let (data_block_offset, data_block_size) = match self.find_in_index_block(&index_block, key)? {
            Some((offset, size)) => (offset, size),
            None => return Ok(None),
        };

        let data_block_data = self.load_block(data_block_offset, data_block_size)?;
        let data_block = Block::new(data_block_data)?;

        self.find_in_data_block(&data_block, key)
    }

    fn find_index_block(&self, key: &[u8]) -> Option<(u64, u32)> {
        // Try ALEX learned index first (O(1) expected)
        let idx = if let Some(ref alex) = self.alex_index {
            let key_i64 = bytes_to_i64(key);
            match alex.get(key_i64) {
                Ok(Some(value)) => {
                    // Decode index position from value
                    if value.len() >= 8 {
                        let mut bytes = [0u8; 8];
                        bytes.copy_from_slice(&value[..8]);
                        u64::from_le_bytes(bytes) as usize
                    } else {
                        // Fall back to binary search on decode error
                        self.top_level_index
                            .binary_search_by(|entry| entry.last_key.as_ref().cmp(key))
                            .unwrap_or_else(|idx| idx)
                    }
                }
                _ => {
                    // ALEX lookup failed - fall back to binary search
                    self.top_level_index
                        .binary_search_by(|entry| entry.last_key.as_ref().cmp(key))
                        .unwrap_or_else(|idx| idx)
                }
            }
        } else {
            // No ALEX index - use binary search
            self.top_level_index
                .binary_search_by(|entry| entry.last_key.as_ref().cmp(key))
                .unwrap_or_else(|idx| idx)
        };

        if idx < self.top_level_index.len() {
            Some((self.top_level_index[idx].offset, self.top_level_index[idx].size))
        } else {
            self.top_level_index.last().map(|e| (e.offset, e.size))
        }
    }

    fn find_in_index_block(&self, index_block: &Block, key: &[u8]) -> Result<Option<(u64, u32)>> {
        for entry in index_block.iter() {
            let (entry_key, entry_value) = entry?;
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

            if key <= entry_key.as_ref() {
                return Ok(Some((offset, size)));
            }
        }

        Ok(None)
    }

    fn find_in_data_block(&mut self, data_block: &Block, key: &[u8]) -> Result<Option<Bytes>> {
        for entry in data_block.iter() {
            let (entry_key, entry_value) = entry?;

            if entry_key.as_ref() == key {
                if entry_value.is_empty() {
                    return Err(SSTableError::InvalidFormat);
                }

                let flag = entry_value[0];
                let data = entry_value.slice(1..);

                match flag {
                    FLAG_INLINE => return Ok(Some(data)),
                    FLAG_POINTER => {
                        if data.len() < 12 {
                            return Err(SSTableError::InvalidFormat);
                        }

                        let offset = u64::from_le_bytes([
                            data[0], data[1], data[2], data[3],
                            data[4], data[5], data[6], data[7],
                        ]);
                        let length = u32::from_le_bytes([
                            data[8], data[9], data[10], data[11],
                        ]);

                        if let Some(ref vlog) = self.vlog {
                            let mut vlog_guard = vlog.lock().unwrap();
                            let pointer = ValuePointer { offset, length };
                            let value = vlog_guard.read(pointer)
                                .map_err(|e| SSTableError::VLog(e.to_string()))?;
                            return Ok(Some(value));
                        } else {
                            return Err(SSTableError::VLog("VLog not attached".to_string()));
                        }
                    }
                    FLAG_TOMBSTONE => return Ok(None),
                    _ => return Err(SSTableError::InvalidFormat),
                }
            }
        }

        Ok(None)
    }

    fn load_block(&self, offset: u64, size: u32) -> Result<Bytes> {
        {
            let cache = self.block_cache.lock().unwrap();
            if let Some(block) = cache.get(&offset) {
                return Ok(block.clone());
            }
        }

        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;

        let mut buf = vec![0u8; size as usize];
        file.read_exact(&mut buf)?;
        let block_data = Bytes::from(buf);

        {
            let mut cache = self.block_cache.lock().unwrap();
            cache.insert(offset, block_data.clone());
        }

        Ok(block_data)
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
            header[16], header[17], header[18], header[19],
            header[20], header[21], header[22], header[23],
        ]);

        Ok((num_entries, 0))
    }

    fn read_footer(file: &mut File) -> Result<(u64, u64)> {
        let file_size = file.metadata()?.len();
        file.seek(SeekFrom::Start(file_size - 40))?;

        let mut footer = [0u8; 40];
        file.read_exact(&mut footer)?;

        let _index_blocks_offset = u64::from_le_bytes([
            footer[0], footer[1], footer[2], footer[3],
            footer[4], footer[5], footer[6], footer[7],
        ]);
        let top_level_offset = u64::from_le_bytes([
            footer[8], footer[9], footer[10], footer[11],
            footer[12], footer[13], footer[14], footer[15],
        ]);
        let bloom_offset = u64::from_le_bytes([
            footer[16], footer[17], footer[18], footer[19],
            footer[20], footer[21], footer[22], footer[23],
        ]);

        let magic = u32::from_le_bytes([footer[28], footer[29], footer[30], footer[31]]);
        let version = u32::from_le_bytes([footer[32], footer[33], footer[34], footer[35]]);

        if magic != MAGIC || version != VERSION {
            return Err(SSTableError::InvalidFormat);
        }

        Ok((top_level_offset, bloom_offset))
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
        // Get file size to validate block offsets
        let file_size = std::fs::metadata(&self.path)?.len();

        // Validate all index blocks and data blocks
        for top_entry in &self.top_level_index.clone() {
            // Check if offset + size is within file bounds
            if top_entry.offset + (top_entry.size as u64) > file_size {
                return Err(SSTableError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Block extends past end of file: offset={}, size={}, file_size={}",
                            top_entry.offset, top_entry.size, file_size),
                )));
            }

            // Load and validate index block
            let index_block_data = self.load_block(top_entry.offset, top_entry.size)?;
            let index_block = Block::new(index_block_data)?;

            // Iterate through index entries to validate data blocks
            for result in index_block.iter() {
                let (_key_bytes, value_bytes) = result?;

                // Index entry format: [key_len: 4][key: variable][offset: 8][size: 4]
                if value_bytes.len() < 16 {
                    continue;
                }

                // Read key_len to skip the key
                let key_len = u32::from_le_bytes(value_bytes[..4].try_into().unwrap()) as usize;
                let offset_start = 4 + key_len;

                if value_bytes.len() < offset_start + 12 {
                    continue;
                }

                let offset = u64::from_le_bytes(value_bytes[offset_start..offset_start+8].try_into().unwrap());
                let size = u32::from_le_bytes(value_bytes[offset_start+8..offset_start+12].try_into().unwrap());

                // Check if data block is within file bounds
                if offset + (size as u64) > file_size {
                    return Err(SSTableError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Data block extends past end of file: offset={}, size={}, file_size={}",
                                offset, size, file_size),
                    )));
                }

                // Load and validate data block
                let data_block_data = self.load_block(offset, size)?;
                let _data_block = Block::new(data_block_data)?;
            }
        }

        Ok(())
    }
}

// ============================================================================
// SSTableIterator - Iterator over all entries
// ============================================================================

pub struct SSTableIterator {
    entries: Vec<(Bytes, Bytes)>,
    position: usize,
}

impl SSTable {
    pub fn iter(&mut self) -> Result<SSTableIterator> {
        let mut entries = Vec::new();

        // Iterate through all data blocks
        for top_entry in &self.top_level_index {
            // Load index block
            let index_block_data = self.load_block(top_entry.offset, top_entry.size)?;
            let index_block = Block::new(index_block_data)?;

            // Iterate through index entries to get data blocks
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

                // Load data block
                let data_block_data = self.load_block(data_offset, data_size)?;
                let data_block = Block::new(data_block_data)?;

                // Iterate through data block entries
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
                                data[0], data[1], data[2], data[3],
                                data[4], data[5], data[6], data[7],
                            ]);
                            let length = u32::from_le_bytes([
                                data[8], data[9], data[10], data[11],
                            ]);

                            if let Some(ref vlog) = self.vlog {
                                let mut vlog_guard = vlog.lock().unwrap();
                                let pointer = ValuePointer { offset, length };
                                vlog_guard.read(pointer)
                                    .map_err(|e| SSTableError::VLog(e.to_string()))?
                            } else {
                                // No vlog attached (e.g., during compaction)
                                // Return the full entry including FLAG_POINTER + pointer bytes
                                // so compaction can preserve vlog pointers
                                entry_value
                            }
                        }
                        FLAG_TOMBSTONE => continue,
                        _ => continue,
                    };

                    entries.push((key, value));
                }
            }
        }

        Ok(SSTableIterator {
            entries,
            position: 0,
        })
    }

    /// Scan a range of keys from this SSTable
    ///
    /// Returns (key, Option<value>) where None indicates a tombstone
    pub fn scan_range(
        &mut self,
        start_key: &[u8],
        end_key: Option<&[u8]>,
    ) -> Result<Vec<(Bytes, Option<Bytes>)>> {
        let mut entries = Vec::new();

        // Iterate through all data blocks
        for top_entry in &self.top_level_index.clone() {
            // Check if this index block might contain keys in our range
            // If last_key < start_key, skip this block entirely
            if top_entry.last_key.as_ref() < start_key {
                continue;
            }

            // Load index block
            let index_block_data = self.load_block(top_entry.offset, top_entry.size)?;
            let index_block = Block::new(index_block_data)?;

            // Iterate through index entries to get data blocks
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

                // Load data block
                let data_block_data = self.load_block(data_offset, data_size)?;
                let data_block = Block::new(data_block_data)?;

                // Iterate through data block entries
                for data_entry in data_block.iter() {
                    let (key, entry_value) = data_entry?;

                    // Check if key is in range
                    if key.as_ref() < start_key {
                        continue;
                    }
                    if let Some(end) = end_key {
                        if key.as_ref() >= end {
                            // Keys are sorted, so we can stop early
                            break;
                        }
                    }

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
                                data[0], data[1], data[2], data[3], data[4], data[5], data[6],
                                data[7],
                            ]);
                            let length = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

                            if let Some(ref vlog) = self.vlog {
                                let mut vlog_guard = vlog.lock().unwrap();
                                let pointer = ValuePointer { offset, length };
                                let value = vlog_guard
                                    .read(pointer)
                                    .map_err(|e| SSTableError::VLog(e.to_string()))?;
                                Some(value)
                            } else {
                                return Err(SSTableError::VLog("VLog not attached".to_string()));
                            }
                        }
                        FLAG_TOMBSTONE => None,
                        _ => continue,
                    };

                    entries.push((key, value_opt));
                }
            }

            // Early exit if we've passed the end_key range
            // (since top_level_index is sorted by last_key)
            if let Some(end) = end_key {
                if top_entry.last_key.as_ref() >= end {
                    break;
                }
            }
        }

        Ok(entries)
    }
}

impl Iterator for SSTableIterator {
    type Item = Result<(Bytes, Bytes)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.entries.len() {
            return None;
        }

        let entry = self.entries[self.position].clone();
        self.position += 1;
        Some(Ok(entry))
    }
}

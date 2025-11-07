// Block-based storage for SSTable
// Implements RocksDB-style block format with prefix compression
//
// Block Structure (4KB default):
// ┌─────────────────────────────────────────────────────┐
// │ Entry 0 (restart): [0][key_len][key][value_len][value] │
// │ Entry 1: [prefix_len][suffix_len][suffix][value_len][value] │
// │ ...                                                  │
// │ Entry 15: [prefix_len][suffix_len][suffix][value_len][value] │
// │ Entry 16 (restart): [0][key_len][key][value_len][value] │
// │ ...                                                  │
// ├─────────────────────────────────────────────────────┤
// │ Restart Points (every 16 entries)                   │
// │ [offset_0: u32][offset_16: u32]...                  │
// ├─────────────────────────────────────────────────────┤
// │ Num Restart Points: u32                             │
// │ Checksum: u32                                        │
// └─────────────────────────────────────────────────────┘
//
// Prefix Compression:
// - Restart points (every 16 entries): Full key stored (prefix_len = 0)
// - Other entries: Share prefix with previous key
// - Format: [prefix_len: u16][suffix_len: u16][suffix][value_len: u32][value]
// - Space savings: 30-50% for keys with common prefixes

use bytes::{Bytes, BytesMut};
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlockError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Block corrupted: checksum mismatch")]
    Corruption,

    #[error("Invalid block format")]
    InvalidFormat,

    #[error("Block full")]
    BlockFull,
}

pub type Result<T> = std::result::Result<T, BlockError>;

/// Default block size (4KB)
pub const DEFAULT_BLOCK_SIZE: usize = 4096;

/// Restart interval for prefix compression (every N entries)
const RESTART_INTERVAL: usize = 16;

/// Block builder for writing entries
pub struct BlockBuilder {
    /// Buffer for block data
    buffer: BytesMut,
    /// Restart points (offsets to full keys)
    restart_points: Vec<u32>,
    /// Number of entries since last restart
    counter: usize,
    /// Last key added (for prefix compression)
    last_key: Bytes,
    /// Maximum block size
    max_size: usize,
}

impl BlockBuilder {
    /// Create a new block builder with default size
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_BLOCK_SIZE)
    }

    /// Create a new block builder with custom capacity
    pub fn with_capacity(max_size: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(max_size),
            restart_points: vec![0], // First entry is always a restart point
            counter: 0,
            last_key: Bytes::new(),
            max_size,
        }
    }

    /// Add an entry to the block
    /// Returns false if block is full
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> bool {
        // Calculate shared prefix length (0 for restart points)
        let prefix_len = if self.counter > 0 && !self.last_key.is_empty() {
            key.iter()
                .zip(self.last_key.iter())
                .take_while(|(a, b)| a == b)
                .count()
        } else {
            0
        };

        let suffix_len = key.len() - prefix_len;

        // Calculate entry size with prefix compression
        // Format: [prefix_len: u16][suffix_len: u16][suffix][value_len: u32][value]
        let entry_size = 2 + 2 + suffix_len + 4 + value.len();

        // Check if we have space (reserve space for footer)
        let footer_size = (self.restart_points.len() + 1) * 4 + 8; // restart_offsets + num_restarts + checksum
        if self.buffer.len() + entry_size + footer_size > self.max_size {
            return false;
        }

        // Check if this should be a restart point
        if self.counter >= RESTART_INTERVAL {
            self.restart_points.push(self.buffer.len() as u32);
            self.counter = 0;
            // Restart point: full key (no prefix compression)
            return self.add(key, value);
        }

        // Write entry with prefix compression
        // [prefix_len: u16][suffix_len: u16][suffix][value_len: u32][value]
        self.buffer.extend_from_slice(&(prefix_len as u16).to_le_bytes());
        self.buffer.extend_from_slice(&(suffix_len as u16).to_le_bytes());
        self.buffer.extend_from_slice(&key[prefix_len..]);
        self.buffer.extend_from_slice(&(value.len() as u32).to_le_bytes());
        self.buffer.extend_from_slice(value);

        self.last_key = Bytes::copy_from_slice(key);
        self.counter += 1;
        true
    }

    /// Get the current size of the block (excluding footer)
    pub fn current_size(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the block is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get the last key added (for index building)
    pub fn last_key(&self) -> &[u8] {
        &self.last_key
    }

    /// Finalize the block and return bytes
    pub fn finish(mut self) -> Bytes {
        // Write restart points
        for offset in &self.restart_points {
            self.buffer.extend_from_slice(&offset.to_le_bytes());
        }

        // Write number of restart points
        self.buffer.extend_from_slice(&(self.restart_points.len() as u32).to_le_bytes());

        // Calculate checksum over data + restart info (hardware-accelerated CRC32C)
        let checksum = crc32c::crc32c(&self.buffer);
        self.buffer.extend_from_slice(&checksum.to_le_bytes());

        self.buffer.freeze()
    }

    /// Reset the builder for reuse
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.restart_points.clear();
        self.restart_points.push(0);
        self.counter = 0;
        self.last_key = Bytes::new();
    }
}

impl Default for BlockBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Block reader for parsing block data
#[derive(Clone)]
pub struct Block {
    data: Bytes,
    restart_offset: usize,
    num_restarts: usize,
}

impl Block {
    /// Parse a block from bytes
    pub fn new(data: Bytes) -> Result<Self> {
        if data.len() < 8 {
            // Minimum: num_restarts(4) + checksum(4)
            return Err(BlockError::InvalidFormat);
        }

        // Read checksum from end
        let stored_checksum = u32::from_le_bytes([
            data[data.len() - 4],
            data[data.len() - 3],
            data[data.len() - 2],
            data[data.len() - 1],
        ]);

        // Verify checksum (everything except checksum itself)
        // Uses hardware-accelerated CRC32C (SSE4.2 on x86, CRC on ARM)
        let computed_checksum = crc32c::crc32c(&data[..data.len() - 4]);

        if stored_checksum != computed_checksum {
            return Err(BlockError::Corruption);
        }

        // Read number of restart points
        let num_restarts = u32::from_le_bytes([
            data[data.len() - 8],
            data[data.len() - 7],
            data[data.len() - 6],
            data[data.len() - 5],
        ]) as usize;

        // Calculate restart offset
        let restart_offset = data.len() - 8 - (num_restarts * 4);

        Ok(Self {
            data,
            restart_offset,
            num_restarts,
        })
    }

    /// Iterate over all entries in the block
    pub fn iter(&self) -> BlockIterator<'_> {
        BlockIterator::new(self)
    }

    /// Get number of entries (approximate - counts restart points)
    pub fn num_entries_approx(&self) -> usize {
        self.num_restarts * RESTART_INTERVAL
    }
}

/// Iterator over block entries
pub struct BlockIterator<'a> {
    block: &'a Block,
    offset: usize,
    /// Last key for prefix reconstruction
    last_key: Bytes,
}

impl<'a> BlockIterator<'a> {
    fn new(block: &'a Block) -> Self {
        Self {
            block,
            offset: 0,
            last_key: Bytes::new(),
        }
    }
}

impl<'a> Iterator for BlockIterator<'a> {
    type Item = Result<(Bytes, Bytes)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.block.restart_offset {
            return None;
        }

        let data = &self.block.data;

        // Read prefix length (u16)
        if self.offset + 2 > self.block.restart_offset {
            return None;
        }
        let prefix_len = u16::from_le_bytes([
            data[self.offset],
            data[self.offset + 1],
        ]) as usize;
        self.offset += 2;

        // Read suffix length (u16)
        if self.offset + 2 > self.block.restart_offset {
            return Some(Err(BlockError::InvalidFormat));
        }
        let suffix_len = u16::from_le_bytes([
            data[self.offset],
            data[self.offset + 1],
        ]) as usize;
        self.offset += 2;

        // Read suffix
        if self.offset + suffix_len > self.block.restart_offset {
            return Some(Err(BlockError::InvalidFormat));
        }
        let suffix = data.slice(self.offset..self.offset + suffix_len);
        self.offset += suffix_len;

        // Reconstruct full key from prefix + suffix
        let key = if prefix_len == 0 {
            // Restart point: suffix is the full key
            suffix.clone()
        } else {
            // Combine prefix from last_key with suffix
            if prefix_len > self.last_key.len() {
                return Some(Err(BlockError::InvalidFormat));
            }
            let mut key_data = BytesMut::with_capacity(prefix_len + suffix_len);
            key_data.extend_from_slice(&self.last_key[..prefix_len]);
            key_data.extend_from_slice(&suffix);
            key_data.freeze()
        };

        // Read value length
        if self.offset + 4 > self.block.restart_offset {
            return Some(Err(BlockError::InvalidFormat));
        }
        let value_len = u32::from_le_bytes([
            data[self.offset],
            data[self.offset + 1],
            data[self.offset + 2],
            data[self.offset + 3],
        ]) as usize;
        self.offset += 4;

        // Read value
        if self.offset + value_len > self.block.restart_offset {
            return Some(Err(BlockError::InvalidFormat));
        }
        let value = data.slice(self.offset..self.offset + value_len);
        self.offset += value_len;

        // Update last_key for next entry
        self.last_key = key.clone();

        Some(Ok((key, value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_builder_single_entry() {
        let mut builder = BlockBuilder::new();
        assert!(builder.add(b"key1", b"value1"));

        let block_data = builder.finish();
        let block = Block::new(block_data).unwrap();

        let entries: Vec<_> = block.iter().collect();
        assert_eq!(entries.len(), 1);

        let (key, value) = entries[0].as_ref().unwrap();
        assert_eq!(key, &Bytes::from("key1"));
        assert_eq!(value, &Bytes::from("value1"));
    }

    #[test]
    fn test_block_builder_multiple_entries() {
        let mut builder = BlockBuilder::new();
        assert!(builder.add(b"key1", b"value1"));
        assert!(builder.add(b"key2", b"value2"));
        assert!(builder.add(b"key3", b"value3"));

        let block_data = builder.finish();
        let block = Block::new(block_data).unwrap();

        let entries: Vec<_> = block.iter().map(|r| r.unwrap()).collect();
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].0, Bytes::from("key1"));
        assert_eq!(entries[1].0, Bytes::from("key2"));
        assert_eq!(entries[2].0, Bytes::from("key3"));
    }

    #[test]
    fn test_block_builder_full() {
        let mut builder = BlockBuilder::with_capacity(256); // Small block

        // Fill until full
        let mut count = 0;
        for i in 0..100 {
            let key = format!("key{:04}", i);
            let value = format!("value{:04}", i);
            if !builder.add(key.as_bytes(), value.as_bytes()) {
                break;
            }
            count += 1;
        }

        assert!(count > 0 && count < 100, "Block should fill before 100 entries");

        let block_data = builder.finish();
        let block = Block::new(block_data).unwrap();

        let entries: Vec<_> = block.iter().collect();
        assert_eq!(entries.len(), count);
    }

    #[test]
    fn test_block_checksum_validation() {
        let mut builder = BlockBuilder::new();
        builder.add(b"key1", b"value1");
        let mut block_data = builder.finish().to_vec();

        // Corrupt a byte
        block_data[0] ^= 0xFF;

        let result = Block::new(Bytes::from(block_data));
        assert!(matches!(result, Err(BlockError::Corruption)));
    }

    #[test]
    fn test_block_restart_points() {
        let mut builder = BlockBuilder::new();

        // Add more than RESTART_INTERVAL entries
        for i in 0..40 {
            let key = format!("key{:04}", i);
            let value = format!("value{:04}", i);
            assert!(builder.add(key.as_bytes(), value.as_bytes()));
        }

        // Should have multiple restart points
        assert!(builder.restart_points.len() > 1);

        let block_data = builder.finish();
        let block = Block::new(block_data).unwrap();

        let entries: Vec<_> = block.iter().map(|r| r.unwrap()).collect();
        assert_eq!(entries.len(), 40);
    }

    #[test]
    fn test_block_large_values() {
        let mut builder = BlockBuilder::new();
        let large_value = vec![b'x'; 2000]; // 2KB value

        assert!(builder.add(b"key1", &large_value));

        let block_data = builder.finish();
        let block = Block::new(block_data).unwrap();

        let entries: Vec<_> = block.iter().collect();
        assert_eq!(entries.len(), 1);

        let (_, value) = entries[0].as_ref().unwrap();
        assert_eq!(value.len(), 2000);
    }
}

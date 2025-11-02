// Value Log (vLog) implementation
// Based on WiscKey paper (USENIX FAST 2016)
//
// Architecture: Append-only log for storing values separately from keys
// - LSM tree stores: key + vLog pointer (offset + length)
// - vLog stores: actual values sequentially
// - Benefit: Compaction only rewrites keys, not values (10-100x less write amp)

use bytes::{Bytes, BytesMut};
use crc32fast::Hasher;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VLogError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("CRC mismatch: expected {expected:x}, got {actual:x}")]
    CrcMismatch { expected: u32, actual: u32 },

    #[error("Invalid record format")]
    InvalidFormat,
}

pub type Result<T> = std::result::Result<T, VLogError>;

/// Value pointer stored in LSM tree
/// Points to value location in vLog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValuePointer {
    /// Offset in vLog file
    pub offset: u64,
    /// Length of value
    pub length: u32,
}

/// Value Log record format:
/// [key_len: u32][key: bytes][value_len: u32][value: bytes][crc: u32]
///
/// Key is stored for GC validation (check if value is still valid)
#[derive(Debug, Clone)]
pub struct VLogRecord {
    pub key: Bytes,
    pub value: Bytes,
}

impl VLogRecord {
    /// Encode record to bytes
    pub fn encode(&self) -> Bytes {
        let key_len = self.key.len() as u32;
        let value_len = self.value.len() as u32;

        let total_len = 4 + key_len as usize + 4 + value_len as usize + 4;
        let mut buf = BytesMut::with_capacity(total_len);

        // Write key
        buf.extend_from_slice(&key_len.to_le_bytes());
        buf.extend_from_slice(&self.key);

        // Write value
        buf.extend_from_slice(&value_len.to_le_bytes());
        buf.extend_from_slice(&self.value);

        // Calculate CRC over key + value
        let mut hasher = Hasher::new();
        hasher.update(&key_len.to_le_bytes());
        hasher.update(&self.key);
        hasher.update(&value_len.to_le_bytes());
        hasher.update(&self.value);
        let crc = hasher.finalize();

        buf.extend_from_slice(&crc.to_le_bytes());

        buf.freeze()
    }

    /// Decode record from bytes
    pub fn decode(data: Bytes) -> Result<Self> {
        if data.len() < 12 {
            // Minimum: key_len(4) + value_len(4) + crc(4)
            return Err(VLogError::InvalidFormat);
        }

        let mut offset = 0;

        // Read key length
        let key_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        offset += 4;

        if offset + key_len > data.len() {
            return Err(VLogError::InvalidFormat);
        }

        // Read key
        let key = data.slice(offset..offset + key_len);
        offset += key_len;

        if offset + 4 > data.len() {
            return Err(VLogError::InvalidFormat);
        }

        // Read value length
        let value_len =
            u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
                as usize;
        offset += 4;

        if offset + value_len + 4 > data.len() {
            return Err(VLogError::InvalidFormat);
        }

        // Read value
        let value = data.slice(offset..offset + value_len);
        offset += value_len;

        // Read CRC
        let stored_crc =
            u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);

        // Verify CRC
        let mut hasher = Hasher::new();
        hasher.update(&(key_len as u32).to_le_bytes());
        hasher.update(&key);
        hasher.update(&(value_len as u32).to_le_bytes());
        hasher.update(&value);
        let computed_crc = hasher.finalize();

        if stored_crc != computed_crc {
            return Err(VLogError::CrcMismatch {
                expected: stored_crc,
                actual: computed_crc,
            });
        }

        Ok(Self { key, value })
    }
}

/// Value Log - append-only log for storing values
pub struct VLog {
    file: File,
    path: PathBuf,
    head: u64, // Append offset (where to write next)
    tail: u64, // GC offset (where GC starts reading)
}

impl VLog {
    /// Create a new vLog
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&path)?;

        Ok(Self {
            file,
            path,
            head: 0,
            tail: 0,
        })
    }

    /// Open an existing vLog
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new().write(true).read(true).open(&path)?;

        let head = file.metadata()?.len();

        Ok(Self {
            file,
            path,
            head,
            tail: 0,
        })
    }

    /// Append a value to the vLog
    /// Returns pointer (offset, length) to be stored in LSM tree
    pub fn append(&mut self, key: &[u8], value: &[u8]) -> Result<ValuePointer> {
        let record = VLogRecord {
            key: Bytes::copy_from_slice(key),
            value: Bytes::copy_from_slice(value),
        };

        let encoded = record.encode();
        let record_offset = self.head;

        // Write to file
        self.file.seek(SeekFrom::Start(self.head))?;
        self.file.write_all(&encoded)?;
        self.file.sync_data()?; // Ensure durability

        self.head += encoded.len() as u64;

        // Return pointer to value (skip key_len + key + value_len prefix)
        let key_len = key.len() as u64;
        let value_offset = record_offset + 4 + key_len + 4;
        let value_len = value.len() as u32;

        Ok(ValuePointer {
            offset: value_offset,
            length: value_len,
        })
    }

    /// Read a value from vLog using pointer
    pub fn read(&mut self, pointer: ValuePointer) -> Result<Bytes> {
        let mut buf = vec![0u8; pointer.length as usize];
        self.file.seek(SeekFrom::Start(pointer.offset))?;
        self.file.read_exact(&mut buf)?;
        Ok(Bytes::from(buf))
    }

    /// Read full record (key + value) at offset
    /// Used for GC validation
    pub fn read_record(&mut self, offset: u64) -> Result<(VLogRecord, u64)> {
        self.file.seek(SeekFrom::Start(offset))?;

        // Read key length
        let mut len_buf = [0u8; 4];
        self.file.read_exact(&mut len_buf)?;
        let key_len = u32::from_le_bytes(len_buf) as usize;

        // Read key
        let mut key_buf = vec![0u8; key_len];
        self.file.read_exact(&mut key_buf)?;

        // Read value length
        self.file.read_exact(&mut len_buf)?;
        let value_len = u32::from_le_bytes(len_buf) as usize;

        // Read value
        let mut value_buf = vec![0u8; value_len];
        self.file.read_exact(&mut value_buf)?;

        // Read CRC
        self.file.read_exact(&mut len_buf)?;

        // Build full record data for decoding
        let total_len = 4 + key_len + 4 + value_len + 4;
        let mut record_data = BytesMut::with_capacity(total_len);
        record_data.extend_from_slice(&(key_len as u32).to_le_bytes());
        record_data.extend_from_slice(&key_buf);
        record_data.extend_from_slice(&(value_len as u32).to_le_bytes());
        record_data.extend_from_slice(&value_buf);
        record_data.extend_from_slice(&len_buf);

        let record = VLogRecord::decode(record_data.freeze())?;
        let next_offset = offset + total_len as u64;

        Ok((record, next_offset))
    }

    /// Get current head position
    pub fn head(&self) -> u64 {
        self.head
    }

    /// Get current tail position
    pub fn tail(&self) -> u64 {
        self.tail
    }

    /// Set tail position (for GC)
    pub fn set_tail(&mut self, tail: u64) {
        self.tail = tail;
    }

    /// Get vLog file size
    pub fn size(&self) -> Result<u64> {
        Ok(self.file.metadata()?.len())
    }

    /// Get vLog path
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_vlog_record_encode_decode() {
        let record = VLogRecord {
            key: Bytes::from("key1"),
            value: Bytes::from("value1"),
        };

        let encoded = record.encode();
        let decoded = VLogRecord::decode(encoded).unwrap();

        assert_eq!(record.key, decoded.key);
        assert_eq!(record.value, decoded.value);
    }

    #[test]
    fn test_vlog_append_and_read() {
        let dir = tempdir().unwrap();
        let vlog_path = dir.path().join("test.vlog");

        let mut vlog = VLog::create(&vlog_path).unwrap();

        // Append value
        let pointer = vlog.append(b"key1", b"value1").unwrap();
        assert_eq!(pointer.length, 6); // "value1".len()

        // Read value back
        let value = vlog.read(pointer).unwrap();
        assert_eq!(value, Bytes::from("value1"));
    }

    #[test]
    fn test_vlog_multiple_values() {
        let dir = tempdir().unwrap();
        let vlog_path = dir.path().join("test.vlog");

        let mut vlog = VLog::create(&vlog_path).unwrap();

        // Append multiple values
        let p1 = vlog.append(b"key1", b"value1").unwrap();
        let p2 = vlog.append(b"key2", b"value2").unwrap();
        let p3 = vlog.append(b"key3", b"value3").unwrap();

        // Read values back
        assert_eq!(vlog.read(p1).unwrap(), Bytes::from("value1"));
        assert_eq!(vlog.read(p2).unwrap(), Bytes::from("value2"));
        assert_eq!(vlog.read(p3).unwrap(), Bytes::from("value3"));
    }

    #[test]
    fn test_vlog_reopen() {
        let dir = tempdir().unwrap();
        let vlog_path = dir.path().join("test.vlog");

        let pointer = {
            let mut vlog = VLog::create(&vlog_path).unwrap();
            vlog.append(b"key1", b"value1").unwrap()
        };

        // Reopen and read
        let mut vlog = VLog::open(&vlog_path).unwrap();
        let value = vlog.read(pointer).unwrap();
        assert_eq!(value, Bytes::from("value1"));
    }

    #[test]
    fn test_vlog_read_record() {
        let dir = tempdir().unwrap();
        let vlog_path = dir.path().join("test.vlog");

        let mut vlog = VLog::create(&vlog_path).unwrap();

        vlog.append(b"key1", b"value1").unwrap();
        vlog.append(b"key2", b"value2").unwrap();

        // Read first record
        let (record, next_offset) = vlog.read_record(0).unwrap();
        assert_eq!(record.key, Bytes::from("key1"));
        assert_eq!(record.value, Bytes::from("value1"));

        // Read second record
        let (record, _) = vlog.read_record(next_offset).unwrap();
        assert_eq!(record.key, Bytes::from("key2"));
        assert_eq!(record.value, Bytes::from("value2"));
    }

    #[test]
    fn test_vlog_large_values() {
        let dir = tempdir().unwrap();
        let vlog_path = dir.path().join("test.vlog");

        let mut vlog = VLog::create(&vlog_path).unwrap();

        // Test with 4KB value (typical embedding size)
        let large_value = vec![b'x'; 4096];
        let pointer = vlog.append(b"key1", &large_value).unwrap();

        let value = vlog.read(pointer).unwrap();
        assert_eq!(value.len(), 4096);
        assert_eq!(value, Bytes::from(large_value));
    }
}

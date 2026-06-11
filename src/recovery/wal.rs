//! Write-Ahead Log (WAL) implementation.
//!
//! The WAL records all mutations before they are applied to the database.
//! Each record is checksummed with CRC32C for integrity.
//!
//! # Record Format
//!
//! ```text
//! [length: u32] [type: u8] [payload: bytes] [crc32c: u4]
//! ```
//!
//! # Sync Policies
//!
//! - `SyncAll`: fsync after every commit (safest, slowest)
//! - `FDataSync`: fdatasync after every commit (good balance)
//! - `None`: no sync (fastest, risk of data loss on crash)

use std::io::{self, Write};

/// Sync policy for the WAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPolicy {
    /// fsync after every commit.
    SyncAll,
    /// fdatasync after every commit.
    FDataSync,
    /// No sync (fastest, risk of data loss).
    None,
}

/// WAL record types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecordType {
    /// PMT update: page_id → (file_id, offset).
    PmtUpdate = 1,
    /// Page allocation.
    PageAlloc = 2,
    /// Page deallocation.
    PageDealloc = 3,
    /// Blob append.
    BlobAppend = 4,
    /// Transaction commit.
    TxnCommit = 5,
    /// Transaction abort.
    TxnAbort = 6,
    /// Checkpoint marker.
    Checkpoint = 7,
}

/// A WAL record.
#[derive(Debug, Clone)]
pub struct WalRecord {
    /// Record type.
    pub record_type: RecordType,
    /// Record payload (variable-length).
    pub payload: Vec<u8>,
}

impl WalRecord {
    /// Create a new WAL record.
    pub fn new(record_type: RecordType, payload: Vec<u8>) -> Self {
        Self { record_type, payload }
    }

    /// Create a PMT update record.
    pub fn pmt_update(page_id: u64, file_id: u32, offset: u64) -> Self {
        let mut payload = Vec::with_capacity(20);
        payload.extend_from_slice(&page_id.to_le_bytes());
        payload.extend_from_slice(&file_id.to_le_bytes());
        payload.extend_from_slice(&offset.to_le_bytes());
        Self::new(RecordType::PmtUpdate, payload)
    }

    /// Create a page allocation record.
    pub fn page_alloc(page_id: u64, file_id: u32) -> Self {
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&page_id.to_le_bytes());
        payload.extend_from_slice(&file_id.to_le_bytes());
        Self::new(RecordType::PageAlloc, payload)
    }

    /// Create a page deallocation record.
    pub fn page_dealloc(page_id: u64) -> Self {
        let mut payload = Vec::with_capacity(8);
        payload.extend_from_slice(&page_id.to_le_bytes());
        Self::new(RecordType::PageDealloc, payload)
    }

    /// Create a transaction commit record.
    pub fn txn_commit(txn_id: u64) -> Self {
        let mut payload = Vec::with_capacity(8);
        payload.extend_from_slice(&txn_id.to_le_bytes());
        Self::new(RecordType::TxnCommit, payload)
    }

    /// Create a transaction abort record.
    pub fn txn_abort(txn_id: u64) -> Self {
        let mut payload = Vec::with_capacity(8);
        payload.extend_from_slice(&txn_id.to_le_bytes());
        Self::new(RecordType::TxnAbort, payload)
    }

    /// Serialize the record to bytes (for writing to the WAL file).
    ///
    /// Format: length(u32) + type(u8) + payload + crc32c(u32)
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload_len = self.payload.len();
        let total_len = 4 + 1 + payload_len + 4; // length + type + payload + crc
        let mut buf = Vec::with_capacity(total_len);

        // Length field (includes type + payload + crc).
        let length = (1 + payload_len + 4) as u32;
        buf.extend_from_slice(&length.to_le_bytes());

        // Record type.
        buf.push(self.record_type as u8);

        // Payload.
        buf.extend_from_slice(&self.payload);

        // CRC32C checksum over type + payload.
        let crc = crc32c::crc32c(&buf[4..]);
        buf.extend_from_slice(&crc.to_le_bytes());

        buf
    }

    /// Deserialize a record from bytes.
    ///
    /// Returns the record and the number of bytes consumed.
    pub fn from_bytes(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() < 4 {
            return None;
        }

        let length = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        let total_len = 4 + length;

        if buf.len() < total_len {
            return None;
        }

        let record_type = match buf[4] {
            1 => RecordType::PmtUpdate,
            2 => RecordType::PageAlloc,
            3 => RecordType::PageDealloc,
            4 => RecordType::BlobAppend,
            5 => RecordType::TxnCommit,
            6 => RecordType::TxnAbort,
            7 => RecordType::Checkpoint,
            _ => return None, // unknown type
        };

        let payload = buf[5..total_len - 4].to_vec();

        // Verify CRC.
        let stored_crc = u32::from_le_bytes([
            buf[total_len - 4],
            buf[total_len - 3],
            buf[total_len - 2],
            buf[total_len - 1],
        ]);
        let computed_crc = crc32c::crc32c(&buf[4..total_len - 4]);
        if stored_crc != computed_crc {
            return None; // CRC mismatch
        }

        Some((Self { record_type, payload }, total_len))
    }
}

/// WAL manager for writing and reading WAL records.
pub struct WalManager {
    /// Buffer for accumulating records before flush.
    buffer: Vec<u8>,
    /// Sync policy.
    sync_policy: SyncPolicy,
    /// Total bytes written.
    bytes_written: u64,
    /// Number of records written.
    records_written: u64,
}

impl WalManager {
    /// Create a new WAL manager with the given sync policy.
    pub fn new(sync_policy: SyncPolicy) -> Self {
        Self {
            buffer: Vec::with_capacity(64 * 1024), // 64KB buffer
            sync_policy,
            bytes_written: 0,
            records_written: 0,
        }
    }

    /// Get the sync policy.
    pub fn sync_policy(&self) -> SyncPolicy {
        self.sync_policy
    }

    /// Get total bytes written.
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Get number of records written.
    pub fn records_written(&self) -> u64 {
        self.records_written
    }

    /// Append a record to the WAL buffer.
    pub fn append(&mut self, record: &WalRecord) {
        let bytes = record.to_bytes();
        self.buffer.extend_from_slice(&bytes);
        self.bytes_written += bytes.len() as u64;
        self.records_written += 1;
    }

    /// Flush the buffer to the provided writer.
    pub fn flush<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        writer.write_all(&self.buffer)?;
        writer.flush()?;
        self.buffer.clear();
        Ok(())
    }

    /// Parse all records from a buffer.
    pub fn parse_records(buf: &[u8]) -> Vec<WalRecord> {
        let mut records = Vec::new();
        let mut pos = 0;

        while pos < buf.len() {
            match WalRecord::from_bytes(&buf[pos..]) {
                Some((record, consumed)) => {
                    records.push(record);
                    pos += consumed;
                }
                None => break, // corrupt or incomplete record
            }
        }

        records
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_roundtrip() {
        let record = WalRecord::pmt_update(42, 0, 4096);
        let bytes = record.to_bytes();
        let (restored, consumed) = WalRecord::from_bytes(&bytes).unwrap();

        assert_eq!(consumed, bytes.len());
        assert_eq!(restored.record_type, RecordType::PmtUpdate);
        assert_eq!(restored.payload, record.payload);
    }

    #[test]
    fn test_record_types() {
        let records = vec![
            WalRecord::pmt_update(1, 0, 100),
            WalRecord::page_alloc(2, 1),
            WalRecord::page_dealloc(3),
            WalRecord::txn_commit(100),
            WalRecord::txn_abort(101),
        ];

        for record in records {
            let bytes = record.to_bytes();
            let (restored, _) = WalRecord::from_bytes(&bytes).unwrap();
            assert_eq!(restored.record_type, record.record_type);
        }
    }

    #[test]
    fn test_wal_manager() {
        let mut wal = WalManager::new(SyncPolicy::FDataSync);

        wal.append(&WalRecord::pmt_update(1, 0, 4096));
        wal.append(&WalRecord::txn_commit(1));

        assert_eq!(wal.records_written(), 2);
        assert!(wal.bytes_written() > 0);
    }

    #[test]
    fn test_wal_flush() {
        let mut wal = WalManager::new(SyncPolicy::None);
        wal.append(&WalRecord::pmt_update(1, 0, 4096));

        let mut buf = Vec::new();
        wal.flush(&mut buf).unwrap();

        assert!(!buf.is_empty());

        // Parse the flushed records.
        let records = WalManager::parse_records(&buf);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record_type, RecordType::PmtUpdate);
    }

    #[test]
    fn test_crc_validation() {
        let record = WalRecord::pmt_update(1, 0, 4096);
        let mut bytes = record.to_bytes();

        // Corrupt the CRC.
        let len = bytes.len();
        bytes[len - 1] ^= 0xFF;

        assert!(WalRecord::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_parse_multiple_records() {
        let mut wal = WalManager::new(SyncPolicy::None);
        wal.append(&WalRecord::pmt_update(1, 0, 100));
        wal.append(&WalRecord::pmt_update(2, 0, 200));
        wal.append(&WalRecord::txn_commit(1));

        let mut buf = Vec::new();
        wal.flush(&mut buf).unwrap();

        let records = WalManager::parse_records(&buf);
        assert_eq!(records.len(), 3);
    }
}

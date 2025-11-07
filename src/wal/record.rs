// WAL record format
// Each record contains: [length][type][data][crc32]

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Checksum mismatch: expected {expected:x}, got {actual:x}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("Invalid record type: {0}")]
    InvalidRecordType(u8),

    #[error("Incomplete record")]
    IncompleteRecord,
}

pub type Result<T> = std::result::Result<T, RecordError>;

#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Put { key: Bytes, value: Bytes },
    Delete { key: Bytes },
}

impl Record {
    /// Encode record to bytes with format:
    /// [length: u32][type: u8][data][crc32: u32]
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();

        match self {
            Record::Put { key, value } => {
                // Type: 1 = Put
                buf.put_u8(1);

                // Key length + key
                buf.put_u32(key.len() as u32);
                buf.put_slice(key);

                // Value length + value
                buf.put_u32(value.len() as u32);
                buf.put_slice(value);
            }
            Record::Delete { key } => {
                // Type: 2 = Delete
                buf.put_u8(2);

                // Key length + key
                buf.put_u32(key.len() as u32);
                buf.put_slice(key);
            }
        }

        // Calculate CRC32C of the data (hardware-accelerated)
        let crc = crc32c::crc32c(&buf);

        // Build final record: [total_length][data][crc32]
        let mut final_buf = BytesMut::new();
        final_buf.put_u32((buf.len() + 4) as u32); // +4 for CRC32
        final_buf.put(buf);
        final_buf.put_u32(crc);

        final_buf.freeze()
    }

    /// Decode record from bytes
    pub fn decode(mut data: Bytes) -> Result<Self> {
        if data.len() < 9 {
            // Minimum: length(4) + type(1) + key_len(4) = 9 bytes
            return Err(RecordError::IncompleteRecord);
        }

        // Read length
        let total_len = data.get_u32() as usize;

        if data.len() < total_len {
            return Err(RecordError::IncompleteRecord);
        }

        // Extract data and CRC
        let data_len = total_len - 4; // Exclude CRC32
        let record_data = data.split_to(data_len);
        let expected_crc = data.get_u32();

        // Verify checksum (hardware-accelerated)
        let actual_crc = crc32c::crc32c(&record_data);
        if actual_crc != expected_crc {
            return Err(RecordError::ChecksumMismatch {
                expected: expected_crc,
                actual: actual_crc,
            });
        }

        // Decode record
        let mut buf = record_data;
        let record_type = buf.get_u8();

        match record_type {
            1 => {
                // Put
                let key_len = buf.get_u32() as usize;
                let key = buf.split_to(key_len);

                let value_len = buf.get_u32() as usize;
                let value = buf.split_to(value_len);

                Ok(Record::Put { key, value })
            }
            2 => {
                // Delete
                let key_len = buf.get_u32() as usize;
                let key = buf.split_to(key_len);

                Ok(Record::Delete { key })
            }
            _ => Err(RecordError::InvalidRecordType(record_type)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_put() {
        let record = Record::Put {
            key: Bytes::from("key1"),
            value: Bytes::from("value1"),
        };

        let encoded = record.encode();
        let decoded = Record::decode(encoded).unwrap();

        assert_eq!(record, decoded);
    }

    #[test]
    fn test_encode_decode_delete() {
        let record = Record::Delete {
            key: Bytes::from("key1"),
        };

        let encoded = record.encode();
        let decoded = Record::decode(encoded).unwrap();

        assert_eq!(record, decoded);
    }

    #[test]
    fn test_checksum_validation() {
        let record = Record::Put {
            key: Bytes::from("key1"),
            value: Bytes::from("value1"),
        };

        let mut encoded = record.encode().to_vec();

        // Corrupt the data
        let len = encoded.len();
        encoded[len - 5] ^= 0xFF;

        let result = Record::decode(Bytes::from(encoded));
        assert!(matches!(result, Err(RecordError::ChecksumMismatch { .. })));
    }
}

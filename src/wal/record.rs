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

/// Operation within a batch record
#[derive(Debug, Clone, PartialEq)]
pub enum BatchOp {
    Put { key: Bytes, value: Bytes },
    Delete { key: Bytes },
    Merge { key: Bytes, operand: Bytes },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Put {
        key: Bytes,
        value: Bytes,
    },
    Delete {
        key: Bytes,
    },
    Merge {
        key: Bytes,
        operand: Bytes,
    },
    /// Atomic batch of operations
    /// All operations in a batch succeed or fail together
    Batch {
        operations: Vec<BatchOp>,
    },
}

impl Record {
    /// Encode record to bytes with format:
    /// [length: u32][type: u8][data][crc32: u32]
    pub fn encode(&self) -> Bytes {
        // Pre-calculate total size to avoid reallocation (single allocation)
        let data_len = match self {
            Record::Put { key, value } => 1 + 4 + key.len() + 4 + value.len(),
            Record::Delete { key } => 1 + 4 + key.len(),
            Record::Merge { key, operand } => 1 + 4 + key.len() + 4 + operand.len(),
            Record::Batch { operations } => {
                let mut len = 1 + 4; // type + count
                for op in operations {
                    len += match op {
                        BatchOp::Put { key, value } => 1 + 4 + key.len() + 4 + value.len(),
                        BatchOp::Delete { key } => 1 + 4 + key.len(),
                        BatchOp::Merge { key, operand } => 1 + 4 + key.len() + 4 + operand.len(),
                    };
                }
                len
            }
        };
        let total_len = 4 + data_len + 4; // length_prefix + data + crc32

        let mut buf = BytesMut::with_capacity(total_len);

        // Write length prefix
        buf.put_u32((data_len + 4) as u32); // data + CRC32

        // Mark data start for CRC calculation
        let data_start = buf.len();

        // Write record data
        match self {
            Record::Put { key, value } => {
                buf.put_u8(1); // Type: Put
                buf.put_u32(key.len() as u32);
                buf.put_slice(key);
                buf.put_u32(value.len() as u32);
                buf.put_slice(value);
            }
            Record::Delete { key } => {
                buf.put_u8(2); // Type: Delete
                buf.put_u32(key.len() as u32);
                buf.put_slice(key);
            }
            Record::Batch { operations } => {
                buf.put_u8(3); // Type: Batch
                buf.put_u32(operations.len() as u32);
                for op in operations {
                    match op {
                        BatchOp::Put { key, value } => {
                            buf.put_u8(1); // Op type: Put
                            buf.put_u32(key.len() as u32);
                            buf.put_slice(key);
                            buf.put_u32(value.len() as u32);
                            buf.put_slice(value);
                        }
                        BatchOp::Delete { key } => {
                            buf.put_u8(2); // Op type: Delete
                            buf.put_u32(key.len() as u32);
                            buf.put_slice(key);
                        }
                        BatchOp::Merge { key, operand } => {
                            buf.put_u8(4); // Op type: Merge
                            buf.put_u32(key.len() as u32);
                            buf.put_slice(key);
                            buf.put_u32(operand.len() as u32);
                            buf.put_slice(operand);
                        }
                    }
                }
            }
            Record::Merge { key, operand } => {
                buf.put_u8(4); // Type: Merge
                buf.put_u32(key.len() as u32);
                buf.put_slice(key);
                buf.put_u32(operand.len() as u32);
                buf.put_slice(operand);
            }
        }

        // Calculate CRC32C over data only (hardware-accelerated)
        let crc = crc32c::crc32c(&buf[data_start..]);
        buf.put_u32(crc);

        buf.freeze()
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
            3 => {
                // Batch
                let count = buf.get_u32() as usize;
                let mut operations = Vec::with_capacity(count);

                for _ in 0..count {
                    let op_type = buf.get_u8();
                    match op_type {
                        1 => {
                            // Put
                            let key_len = buf.get_u32() as usize;
                            let key = buf.split_to(key_len);

                            let value_len = buf.get_u32() as usize;
                            let value = buf.split_to(value_len);

                            operations.push(BatchOp::Put { key, value });
                        }
                        2 => {
                            // Delete
                            let key_len = buf.get_u32() as usize;
                            let key = buf.split_to(key_len);

                            operations.push(BatchOp::Delete { key });
                        }
                        4 => {
                            // Merge
                            let key_len = buf.get_u32() as usize;
                            let key = buf.split_to(key_len);

                            let operand_len = buf.get_u32() as usize;
                            let operand = buf.split_to(operand_len);

                            operations.push(BatchOp::Merge { key, operand });
                        }
                        _ => return Err(RecordError::InvalidRecordType(op_type)),
                    }
                }

                Ok(Record::Batch { operations })
            }
            4 => {
                // Merge
                let key_len = buf.get_u32() as usize;
                let key = buf.split_to(key_len);

                let operand_len = buf.get_u32() as usize;
                let operand = buf.split_to(operand_len);

                Ok(Record::Merge { key, operand })
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
    fn test_encode_decode_merge() {
        let record = Record::Merge {
            key: Bytes::from("key1"),
            operand: Bytes::from("op1"),
        };

        let encoded = record.encode();
        let decoded = Record::decode(encoded).unwrap();

        assert_eq!(record, decoded);
    }

    #[test]
    fn test_encode_decode_batch() {
        let record = Record::Batch {
            operations: vec![
                BatchOp::Put {
                    key: Bytes::from("key1"),
                    value: Bytes::from("val1"),
                },
                BatchOp::Merge {
                    key: Bytes::from("key1"),
                    operand: Bytes::from("op2"),
                },
                BatchOp::Delete {
                    key: Bytes::from("key2"),
                },
            ],
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

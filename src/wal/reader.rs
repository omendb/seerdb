// WAL reader for crash recovery

use super::record::{Record, RecordError};
use bytes::BytesMut;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReaderError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Record error: {0}")]
    Record(#[from] RecordError),
}

pub type Result<T> = std::result::Result<T, ReaderError>;

/// WAL reader for crash recovery
pub struct WALReader {
    file: File,
    offset: u64,
}

impl WALReader {
    /// Open a WAL file for reading
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path)?;
        Ok(Self { file, offset: 0 })
    }

    /// Read the next record from the WAL
    /// Returns None if end of file reached
    pub fn read_next(&mut self) -> Result<Option<Record>> {
        // Try to read length prefix (4 bytes)
        let mut len_buf = [0u8; 4];
        match self.file.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                // End of file reached
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }

        let total_len = u32::from_be_bytes(len_buf) as usize;

        // Read the rest of the record (data + CRC)
        let mut record_buf = BytesMut::with_capacity(4 + total_len);
        record_buf.extend_from_slice(&len_buf);

        let mut data_buf = vec![0u8; total_len];
        self.file.read_exact(&mut data_buf)?;
        record_buf.extend_from_slice(&data_buf);

        self.offset += (4 + total_len) as u64;

        // Decode record
        let record = Record::decode(record_buf.freeze())?;
        Ok(Some(record))
    }

    /// Read all records from the WAL
    pub fn read_all(&mut self) -> Result<Vec<Record>> {
        let mut records = Vec::new();

        while let Some(record) = self.read_next()? {
            records.push(record);
        }

        Ok(records)
    }

    /// Seek to a specific offset in the WAL
    pub fn seek(&mut self, offset: u64) -> Result<()> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.offset = offset;
        Ok(())
    }

    /// Get the current offset
    pub fn offset(&self) -> u64 {
        self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::{Record, SyncPolicy, WAL};
    use bytes::Bytes;
    use tempfile::tempdir;

    #[test]
    fn test_reader_read_all() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test.wal");

        // Write some records
        {
            let mut wal = WAL::create(&wal_path, SyncPolicy::SyncAll).unwrap();
            let records = vec![
                Record::Put {
                    key: Bytes::from("key1"),
                    value: Bytes::from("value1"),
                },
                Record::Put {
                    key: Bytes::from("key2"),
                    value: Bytes::from("value2"),
                },
                Record::Delete {
                    key: Bytes::from("key1"),
                },
            ];
            wal.write_batch(&records).unwrap();
        }

        // Read them back
        let mut reader = WALReader::open(&wal_path).unwrap();
        let records = reader.read_all().unwrap();

        assert_eq!(records.len(), 3);
        assert!(matches!(records[0], Record::Put { .. }));
        assert!(matches!(records[1], Record::Put { .. }));
        assert!(matches!(records[2], Record::Delete { .. }));
    }

    #[test]
    fn test_reader_read_next() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test.wal");

        // Write records
        {
            let mut wal = WAL::create(&wal_path, SyncPolicy::SyncAll).unwrap();
            wal.write(&Record::Put {
                key: Bytes::from("key1"),
                value: Bytes::from("value1"),
            })
            .unwrap();
        }

        // Read one by one
        let mut reader = WALReader::open(&wal_path).unwrap();
        let record1 = reader.read_next().unwrap();
        assert!(record1.is_some());

        let record2 = reader.read_next().unwrap();
        assert!(record2.is_none()); // EOF
    }
}

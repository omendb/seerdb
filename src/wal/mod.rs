// Write-Ahead Log (WAL) implementation
// Provides durability guarantees for memtable operations

pub mod record;
pub mod reader;

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub use record::Record;
pub use reader::WALReader;

#[derive(Debug, Error)]
pub enum WALError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Record error: {0}")]
    Record(#[from] record::RecordError),
}

pub type Result<T> = std::result::Result<T, WALError>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncPolicy {
    /// Sync data and metadata on every write (safest, slowest)
    SyncAll,
    /// Sync data only on every write (safe, faster)
    SyncData,
    /// No sync, rely on OS (fastest, least safe)
    None,
}

/// Write-Ahead Log writer
pub struct WAL {
    file: Arc<Mutex<File>>,
    path: PathBuf,
    offset: u64,
    sync_policy: SyncPolicy,
}

impl WAL {
    /// Create a new WAL file
    pub fn create(path: impl AsRef<Path>, sync_policy: SyncPolicy) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .create(true)
            
            .append(true)
            .open(&path)?;

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            path,
            offset: 0,
            sync_policy,
        })
    }

    /// Open an existing WAL file
    pub fn open(path: impl AsRef<Path>, sync_policy: SyncPolicy) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new().append(true).open(&path)?;

        let offset = file.metadata()?.len();

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            path,
            offset,
            sync_policy,
        })
    }

    /// Write a record to the WAL
    pub fn write(&mut self, record: &Record) -> Result<u64> {
        let encoded = record.encode();
        let record_offset = self.offset;

        let mut file = self.file.lock().expect("WAL file mutex poisoned");
        file.write_all(&encoded)?;

        // Sync based on policy
        match self.sync_policy {
            SyncPolicy::SyncAll => file.sync_all()?,
            SyncPolicy::SyncData => file.sync_data()?,
            SyncPolicy::None => {}
        }

        self.offset += encoded.len() as u64;

        Ok(record_offset)
    }

    /// Write a batch of records
    pub fn write_batch(&mut self, records: &[Record]) -> Result<Vec<u64>> {
        let mut offsets = Vec::with_capacity(records.len());

        {
            let mut file = self.file.lock().expect("WAL file mutex poisoned");
            for record in records {
                let encoded = record.encode();
                offsets.push(self.offset);

                file.write_all(&encoded)?;
                self.offset += encoded.len() as u64;
            }

            // Sync once at the end for batch
            match self.sync_policy {
                SyncPolicy::SyncAll => file.sync_all()?,
                SyncPolicy::SyncData => file.sync_data()?,
                SyncPolicy::None => {}
            }
        }

        Ok(offsets)
    }

    /// Get the current offset (end of file)
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Get the file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Sync the WAL to disk
    pub fn sync(&self) -> Result<()> {
        let file = self.file.lock().expect("WAL file mutex poisoned");
        file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tempfile::tempdir;

    #[test]
    fn test_wal_create_and_write() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test.wal");

        let mut wal = WAL::create(&wal_path, SyncPolicy::SyncAll).unwrap();

        let record = Record::Put {
            key: Bytes::from("key1"),
            value: Bytes::from("value1"),
        };

        let offset = wal.write(&record).unwrap();
        assert_eq!(offset, 0);

        assert!(wal_path.exists());
    }

    #[test]
    fn test_wal_write_batch() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test.wal");

        let mut wal = WAL::create(&wal_path, SyncPolicy::SyncData).unwrap();

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

        let offsets = wal.write_batch(&records).unwrap();
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets[0], 0);
    }

    #[test]
    fn test_wal_reopen() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test.wal");

        {
            let mut wal = WAL::create(&wal_path, SyncPolicy::SyncAll).unwrap();
            let record = Record::Put {
                key: Bytes::from("key1"),
                value: Bytes::from("value1"),
            };
            wal.write(&record).unwrap();
        }

        let wal = WAL::open(&wal_path, SyncPolicy::SyncAll).unwrap();
        assert!(wal.offset() > 0);
    }
}

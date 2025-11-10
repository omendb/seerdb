// Storage backend abstraction for pluggable storage tiers
//
// Feature-gated: S3/object storage backends only compiled when --features s3-backend
// Default (no feature): Uses concrete LocalDiskBackend (zero overhead)
// With feature: Generic backend trait (monomorphized = zero overhead)

use crate::db::Result;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Storage trait for pluggable storage implementations
///
/// Enables:
/// - Local disk (default, always available)
/// - Object storage: S3, GCS, Azure, R2 (requires --features s3-backend)
/// - Custom backends (encryption, compression, tiering)
///
/// Performance: Monomorphized generics ensure zero runtime overhead
///
/// # Zero-Cost Abstraction
///
/// When `s3-backend` feature is disabled (default):
/// - No trait overhead, direct function calls
/// - LocalStorage methods are inlined by compiler
/// - Binary size identical to hand-written file I/O
///
/// When `s3-backend` feature is enabled:
/// - Trait uses monomorphized generics (static dispatch)
/// - Compiler optimizes as if no trait exists
/// - Zero runtime cost vs direct implementation
#[cfg(feature = "s3-backend")]
pub trait Storage: Send + Sync {
    /// Read a block from an SSTable at the given offset
    ///
    /// Returns the raw bytes (caller handles decompression/parsing)
    fn read_block(&self, path: &Path, offset: u64, size: u32) -> Result<Vec<u8>>;

    /// Write an SSTable to storage
    ///
    /// Data should be fully buffered by caller before calling this
    fn write_sstable(&self, path: &Path, data: &[u8]) -> Result<()>;

    /// Delete an SSTable from storage
    fn delete_sstable(&self, path: &Path) -> Result<()>;

    /// Fsync an SSTable to ensure durability
    fn sync(&self, path: &Path) -> Result<()>;

    /// Check if an SSTable exists
    fn exists(&self, path: &Path) -> Result<bool>;

    /// List all SSTables in a directory
    fn list_sstables(&self, dir: &Path) -> Result<Vec<PathBuf>>;
}

/// Local disk storage implementation
///
/// Direct file system operations with zero abstraction overhead.
///
/// # Performance
///
/// All methods are inlined by the compiler when possible, resulting in
/// performance identical to hand-written file I/O code.
pub struct LocalStorage {
    base_path: PathBuf,
}

impl LocalStorage {
    /// Create a new local storage instance
    ///
    /// # Arguments
    ///
    /// * `base_path` - Base directory for all storage operations
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Get the full path for a relative path
    fn full_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_path.join(path)
        }
    }
}

/// Implement Storage trait when S3 feature is enabled
#[cfg(feature = "s3-backend")]
impl Storage for LocalStorage {
    fn read_block(&self, path: &Path, offset: u64, size: u32) -> Result<Vec<u8>> {
        let full_path = self.full_path(path);
        let mut file = File::open(&full_path)?;
        file.seek(SeekFrom::Start(offset))?;

        let mut buffer = vec![0u8; size as usize];
        file.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    fn write_sstable(&self, path: &Path, data: &[u8]) -> Result<()> {
        let full_path = self.full_path(path);

        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&full_path)?;

        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    }

    fn delete_sstable(&self, path: &Path) -> Result<()> {
        let full_path = self.full_path(path);
        std::fs::remove_file(full_path)?;
        Ok(())
    }

    fn sync(&self, path: &Path) -> Result<()> {
        let full_path = self.full_path(path);
        let file = OpenOptions::new().write(true).open(full_path)?;
        file.sync_all()?;
        Ok(())
    }

    fn exists(&self, path: &Path) -> Result<bool> {
        let full_path = self.full_path(path);
        Ok(full_path.exists())
    }

    fn list_sstables(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let full_dir = self.full_path(dir);
        let mut sstables = Vec::new();

        if full_dir.exists() && full_dir.is_dir() {
            for entry in std::fs::read_dir(&full_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("sst") {
                    sstables.push(path);
                }
            }
        }

        Ok(sstables)
    }
}

// When S3 feature is disabled: LocalStorage is standalone (no trait)
// This avoids trait overhead and keeps binary size minimal
#[cfg(not(feature = "s3-backend"))]
impl LocalStorage {
    /// Read a block from an SSTable (direct implementation, no trait)
    pub fn read_block(&self, path: &Path, offset: u64, size: u32) -> Result<Vec<u8>> {
        let full_path = self.full_path(path);
        let mut file = File::open(&full_path)?;
        file.seek(SeekFrom::Start(offset))?;

        let mut buffer = vec![0u8; size as usize];
        file.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    /// Write an SSTable (direct implementation, no trait)
    pub fn write_sstable(&self, path: &Path, data: &[u8]) -> Result<()> {
        let full_path = self.full_path(path);

        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&full_path)?;

        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    }

    /// Delete an SSTable (direct implementation, no trait)
    pub fn delete_sstable(&self, path: &Path) -> Result<()> {
        let full_path = self.full_path(path);
        std::fs::remove_file(full_path)?;
        Ok(())
    }

    /// Fsync an SSTable (direct implementation, no trait)
    pub fn sync(&self, path: &Path) -> Result<()> {
        let full_path = self.full_path(path);
        let file = OpenOptions::new().write(true).open(full_path)?;
        file.sync_all()?;
        Ok(())
    }

    /// Check if an SSTable exists (direct implementation, no trait)
    pub fn exists(&self, path: &Path) -> Result<bool> {
        let full_path = self.full_path(path);
        Ok(full_path.exists())
    }

    /// List all SSTables (direct implementation, no trait)
    pub fn list_sstables(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let full_dir = self.full_path(dir);
        let mut sstables = Vec::new();

        if full_dir.exists() && full_dir.is_dir() {
            for entry in std::fs::read_dir(&full_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("sst") {
                    sstables.push(path);
                }
            }
        }

        Ok(sstables)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_local_storage_write_read() {
        let dir = tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf());

        // Write data
        let path = Path::new("test.sst");
        let data = b"hello world";
        storage.write_sstable(path, data).unwrap();

        // Read back
        let read_data = storage.read_block(path, 0, data.len() as u32).unwrap();
        assert_eq!(&read_data, data);
    }

    #[test]
    fn test_local_storage_exists() {
        let dir = tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf());

        let path = Path::new("test.sst");
        assert!(!storage.exists(path).unwrap());

        storage.write_sstable(path, b"data").unwrap();
        assert!(storage.exists(path).unwrap());
    }

    #[test]
    fn test_local_storage_delete() {
        let dir = tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf());

        let path = Path::new("test.sst");
        storage.write_sstable(path, b"data").unwrap();
        assert!(storage.exists(path).unwrap());

        storage.delete_sstable(path).unwrap();
        assert!(!storage.exists(path).unwrap());
    }

    #[test]
    fn test_local_storage_list() {
        let dir = tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf());

        // Write multiple SSTables
        storage.write_sstable(Path::new("L0_001.sst"), b"data1").unwrap();
        storage.write_sstable(Path::new("L0_002.sst"), b"data2").unwrap();
        storage.write_sstable(Path::new("L1_001.sst"), b"data3").unwrap();

        // List all
        let sstables = storage.list_sstables(Path::new(".")).unwrap();
        assert_eq!(sstables.len(), 3);
    }
}

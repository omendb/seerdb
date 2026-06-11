//! Device abstraction for file I/O.
//!
//! On Linux, supports O_DIRECT for bypassing the page cache.
//! On macOS and other platforms, falls back to buffered I/O.

use crate::btree::node::PAGE_SIZE;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Options for opening a device.
#[derive(Debug, Clone)]
pub struct DeviceOptions {
    /// Use O_DIRECT on Linux (bypass page cache).
    pub use_odirect: bool,
    /// Use fsync after writes.
    pub sync_writes: bool,
    /// Create the file if it doesn't exist.
    pub create: bool,
}

impl Default for DeviceOptions {
    fn default() -> Self {
        Self {
            use_odirect: true,
            sync_writes: false,
            create: true,
        }
    }
}

/// A device (file) for storing pages.
///
/// Handles page-aligned I/O and optionally uses O_DIRECT on Linux.
pub struct Device {
    /// The underlying file.
    file: File,
    /// Whether to use O_DIRECT.
    use_odirect: bool,
    /// Whether to sync after writes.
    sync_writes: bool,
}

impl Device {
    /// Open a device file.
    pub fn open<P: AsRef<Path>>(path: P, options: &DeviceOptions) -> io::Result<Self> {
        let mut open_options = OpenOptions::new();
        open_options.read(true).write(true);

        if options.create {
            open_options.create(true);
        }

        #[cfg(target_os = "linux")]
        if options.use_odirect {
            use std::os::unix::fs::OpenOptionsExt;
            open_options.custom_flags(libc::O_DIRECT);
        }

        let file = open_options.open(path)?;

        Ok(Self {
            file,
            use_odirect: options.use_odirect,
            sync_writes: options.sync_writes,
        })
    }

    /// Read a page at the given offset.
    ///
    /// The buffer must be page-aligned for O_DIRECT.
    pub fn read_page(&mut self, offset: u64, buf: &mut [u8; PAGE_SIZE]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buf)?;
        Ok(())
    }

    /// Write a page at the given offset.
    ///
    /// The buffer must be page-aligned for O_DIRECT.
    pub fn write_page(&mut self, offset: u64, buf: &[u8; PAGE_SIZE]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(buf)?;

        if self.sync_writes {
            self.file.sync_data()?;
        }

        Ok(())
    }

    /// Sync all data to disk.
    pub fn sync(&self) -> io::Result<()> {
        self.file.sync_data()
    }

    /// Get the file size.
    pub fn size(&self) -> io::Result<u64> {
        self.file.metadata().map(|m| m.len())
    }

    /// Whether O_DIRECT is being used.
    pub fn uses_odirect(&self) -> bool {
        self.use_odirect
    }
}

/// Allocate a page-aligned buffer for O_DIRECT I/O.
///
/// Returns a buffer of the given size, aligned to PAGE_SIZE.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn alloc_aligned_buffer(size: usize) -> Vec<u8> {
    use std::alloc::{alloc_zeroed, Layout};

    let layout = Layout::from_size_align(size, PAGE_SIZE).expect("invalid layout");
    let ptr = unsafe { alloc_zeroed(layout) };
    if ptr.is_null() {
        panic!("failed to allocate aligned buffer");
    }
    unsafe { Vec::from_raw_parts(ptr, size, size) }
}

/// Allocate a page-aligned buffer (non-Linux fallback).
#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub fn alloc_aligned_buffer(size: usize) -> Vec<u8> {
    // On non-Linux, we don't need strict alignment, but provide it for consistency.
    vec![0u8; size]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_device_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let options = DeviceOptions::default();

        let device = Device::open(&path, &options);
        assert!(device.is_ok());
    }

    #[test]
    fn test_device_read_write() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let options = DeviceOptions {
            use_odirect: false, // disable O_DIRECT for testing
            sync_writes: false,
            create: true,
        };

        let mut device = Device::open(&path, &options).unwrap();

        let write_buf = [42u8; PAGE_SIZE];
        device.write_page(0, &write_buf).unwrap();

        let mut read_buf = [0u8; PAGE_SIZE];
        device.read_page(0, &mut read_buf).unwrap();

        assert_eq!(read_buf, write_buf);
    }

    #[test]
    fn test_device_multiple_pages() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let options = DeviceOptions {
            use_odirect: false,
            sync_writes: false,
            create: true,
        };

        let mut device = Device::open(&path, &options).unwrap();

        // Write multiple pages.
        for i in 0..10 {
            let buf = [i as u8; PAGE_SIZE];
            device.write_page(i as u64 * PAGE_SIZE as u64, &buf).unwrap();
        }

        // Read them back.
        for i in 0..10 {
            let mut buf = [0u8; PAGE_SIZE];
            device.read_page(i as u64 * PAGE_SIZE as u64, &mut buf).unwrap();
            assert_eq!(buf, [i as u8; PAGE_SIZE]);
        }
    }

    #[test]
    fn test_device_size() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let options = DeviceOptions {
            use_odirect: false,
            sync_writes: false,
            create: true,
        };

        let mut device = Device::open(&path, &options).unwrap();
        assert_eq!(device.size().unwrap(), 0);

        let buf = [0u8; PAGE_SIZE];
        device.write_page(0, &buf).unwrap();
        assert_eq!(device.size().unwrap(), PAGE_SIZE as u64);
    }

    #[test]
    fn test_aligned_buffer() {
        let buf = alloc_aligned_buffer(PAGE_SIZE);
        assert_eq!(buf.len(), PAGE_SIZE);
        // On Linux with O_DIRECT, alignment is guaranteed.
        // On other platforms, we just check the size.
        #[cfg(target_os = "linux")]
        assert_eq!(buf.as_ptr() as usize % PAGE_SIZE, 0);
    }
}

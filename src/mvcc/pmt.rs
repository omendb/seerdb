//! Page Mapping Table (PMT).
//!
//! The PMT is an in-memory hash map that tracks the current location of each
//! page in the out-of-place B-tree. When a page is written (out-of-place),
//! the PMT is updated to point to the new location.
//!
//! # Format
//!
//! ```text
//! page_id → PageMapping { file_id, offset, version }
//! ```
//!
//! The PMT is persisted via the WAL for crash recovery.

use std::collections::HashMap;

/// Location of a page on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageMapping {
    /// File containing the page (0 for the main data file).
    pub file_id: u32,
    /// Byte offset within the file.
    pub offset: u64,
    /// Version number (incremented on each out-of-place write).
    pub version: u64,
}

impl PageMapping {
    /// Create a new page mapping.
    pub fn new(file_id: u32, offset: u64, version: u64) -> Self {
        Self {
            file_id,
            offset,
            version,
        }
    }

    /// Size of the serialized mapping (file_id:4 + offset:8 + version:8 = 20 bytes).
    pub const SERIALIZED_SIZE: usize = 20;

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> [u8; Self::SERIALIZED_SIZE] {
        let mut buf = [0u8; Self::SERIALIZED_SIZE];
        buf[0..4].copy_from_slice(&self.file_id.to_le_bytes());
        buf[4..12].copy_from_slice(&self.offset.to_le_bytes());
        buf[12..20].copy_from_slice(&self.version.to_le_bytes());
        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(buf: &[u8; Self::SERIALIZED_SIZE]) -> Self {
        let file_id = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let offset = u64::from_le_bytes([
            buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11],
        ]);
        let version = u64::from_le_bytes([
            buf[12], buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19],
        ]);
        Self { file_id, offset, version }
    }
}

/// Page Mapping Table: maps page IDs to their current on-disk locations.
///
/// The PMT is the core of the out-of-place B-tree design. When a page is
/// modified, a new version is written to a different location, and the PMT
/// is updated atomically to point to the new location.
pub struct PMT {
    /// The mapping table.
    mappings: HashMap<u64, PageMapping>,
    /// Next version number for new mappings.
    next_version: u64,
}

impl PMT {
    /// Create a new empty PMT.
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
            next_version: 1,
        }
    }

    /// Get the mapping for a page.
    pub fn get(&self, page_id: u64) -> Option<&PageMapping> {
        self.mappings.get(&page_id)
    }

    /// Insert or update a page mapping.
    ///
    /// Returns the old mapping if the page was previously mapped.
    pub fn insert(&mut self, page_id: u64, file_id: u32, offset: u64) -> Option<PageMapping> {
        let mapping = PageMapping::new(file_id, offset, self.next_version);
        self.next_version += 1;
        self.mappings.insert(page_id, mapping)
    }

    /// Remove a page mapping (page is being deleted/freed).
    ///
    /// Returns the old mapping if the page was previously mapped.
    pub fn remove(&mut self, page_id: u64) -> Option<PageMapping> {
        self.mappings.remove(&page_id)
    }

    /// Check if a page is mapped.
    pub fn contains(&self, page_id: u64) -> bool {
        self.mappings.contains_key(&page_id)
    }

    /// Number of mapped pages.
    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    /// Whether the PMT is empty.
    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }

    /// Iterate over all mappings.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &PageMapping)> {
        self.mappings.iter().map(|(&k, v)| (k, v))
    }

    /// Serialize all mappings to bytes (for WAL persistence).
    ///
    /// Format: count(u32) followed by count × (page_id:u64, mapping:20 bytes)
    pub fn to_bytes(&self) -> Vec<u8> {
        let count = self.mappings.len() as u32;
        let mut buf = Vec::with_capacity(4 + self.len() * (8 + PageMapping::SERIALIZED_SIZE));
        buf.extend_from_slice(&count.to_le_bytes());

        for (&page_id, mapping) in &self.mappings {
            buf.extend_from_slice(&page_id.to_le_bytes());
            buf.extend_from_slice(&mapping.to_bytes());
        }

        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        if buf.len() < 4 {
            return None;
        }

        let count = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        let expected_size = 4 + count * (8 + PageMapping::SERIALIZED_SIZE);
        if buf.len() < expected_size {
            return None;
        }

        let mut pmt = Self::new();
        let mut pos = 4;

        for _ in 0..count {
            let page_id = u64::from_le_bytes([
                buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3],
                buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7],
            ]);
            pos += 8;

            let mut mapping_buf = [0u8; PageMapping::SERIALIZED_SIZE];
            mapping_buf.copy_from_slice(&buf[pos..pos + PageMapping::SERIALIZED_SIZE]);
            let mapping = PageMapping::from_bytes(&mapping_buf);
            pos += PageMapping::SERIALIZED_SIZE;

            // Track the highest version.
            if mapping.version >= pmt.next_version {
                pmt.next_version = mapping.version + 1;
            }

            pmt.mappings.insert(page_id, mapping);
        }

        Some(pmt)
    }
}

impl Default for PMT {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pmt_insert_get() {
        let mut pmt = PMT::new();
        pmt.insert(1, 0, 4096);

        let mapping = pmt.get(1).unwrap();
        assert_eq!(mapping.file_id, 0);
        assert_eq!(mapping.offset, 4096);
        assert_eq!(mapping.version, 1);
    }

    #[test]
    fn test_pmt_update() {
        let mut pmt = PMT::new();
        pmt.insert(1, 0, 4096);
        pmt.insert(1, 0, 8192);

        let mapping = pmt.get(1).unwrap();
        assert_eq!(mapping.offset, 8192);
        assert_eq!(mapping.version, 2);
    }

    #[test]
    fn test_pmt_remove() {
        let mut pmt = PMT::new();
        pmt.insert(1, 0, 4096);

        let old = pmt.remove(1).unwrap();
        assert_eq!(old.offset, 4096);
        assert!(pmt.get(1).is_none());
    }

    #[test]
    fn test_pmt_contains() {
        let mut pmt = PMT::new();
        assert!(!pmt.contains(1));

        pmt.insert(1, 0, 4096);
        assert!(pmt.contains(1));
    }

    #[test]
    fn test_pmt_serialization() {
        let mut pmt = PMT::new();
        pmt.insert(1, 0, 4096);
        pmt.insert(2, 1, 8192);
        pmt.insert(3, 0, 12288);

        let bytes = pmt.to_bytes();
        let restored = PMT::from_bytes(&bytes).unwrap();

        assert_eq!(restored.len(), 3);
        assert_eq!(restored.get(1).unwrap().offset, 4096);
        assert_eq!(restored.get(2).unwrap().file_id, 1);
        assert_eq!(restored.get(3).unwrap().version, 3);
    }

    #[test]
    fn test_pmt_version_tracking() {
        let mut pmt = PMT::new();
        assert_eq!(pmt.next_version, 1);

        pmt.insert(1, 0, 0);
        assert_eq!(pmt.next_version, 2);

        pmt.insert(2, 0, 0);
        assert_eq!(pmt.next_version, 3);
    }

    #[test]
    fn test_pmt_iter() {
        let mut pmt = PMT::new();
        pmt.insert(1, 0, 100);
        pmt.insert(2, 0, 200);

        let mut entries: Vec<_> = pmt.iter().collect();
        entries.sort_by_key(|(id, _)| *id);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, 1);
        assert_eq!(entries[1].0, 2);
    }
}

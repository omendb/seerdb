// LSM-Tree Compaction
// Implements leveled compaction to keep read amplification bounded

pub mod merge;

use crate::sstable::{SSTable, SSTableBuilder};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use merge::MergeIterator;

#[derive(Debug, Error)]
pub enum CompactionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSTable error: {0}")]
    SSTable(#[from] crate::sstable::SSTableError),

    #[error("No SSTables to compact")]
    NoInput,
}

pub type Result<T> = std::result::Result<T, CompactionError>;

/// Compact multiple SSTables into a single SSTable
///
/// # Arguments
/// * `input_paths` - Paths to SSTables to compact (newer first)
/// * `output_path` - Path for output SSTable
///
/// # Returns
/// Path to the new SSTable and its size in bytes
pub fn compact_sstables(
    input_paths: &[PathBuf],
    output_path: impl AsRef<Path>,
) -> Result<(PathBuf, u64)> {
    if input_paths.is_empty() {
        return Err(CompactionError::NoInput);
    }

    // Open all input SSTables
    let mut sstables = Vec::new();
    for path in input_paths {
        let sstable = SSTable::open(path)?;
        sstables.push(sstable);
    }

    // Create merge iterator
    let merge = MergeIterator::new(sstables)?;

    // Build new SSTable from merged entries
    let output_path = output_path.as_ref().to_path_buf();
    let mut builder = SSTableBuilder::create(&output_path)?;

    for result in merge {
        let (key, value) = result?;
        builder.add(key, value)?;
    }

    // Finish writing
    builder.finish()?;

    // Get file size
    let metadata = std::fs::metadata(&output_path)?;
    let size = metadata.len();

    Ok((output_path, size))
}

/// Represents a level in the LSM tree
#[derive(Debug)]
pub struct Level {
    /// Level number (0 = memtable flush target, 1+ = compacted levels)
    level_num: usize,
    /// SSTables in this level (sorted by key range)
    sstables: Vec<PathBuf>,
    /// Current size in bytes
    size: u64,
    /// Size threshold for triggering compaction
    size_threshold: u64,
}

impl Level {
    /// Create a new level with given threshold
    pub fn new(level_num: usize, size_threshold: u64) -> Self {
        Self {
            level_num,
            sstables: Vec::new(),
            size: 0,
            size_threshold,
        }
    }

    /// Add an SSTable to this level
    pub fn add_sstable(&mut self, path: PathBuf, size: u64) {
        self.sstables.push(path);
        self.size += size;
    }

    /// Check if this level needs compaction
    pub fn needs_compaction(&self) -> bool {
        self.size >= self.size_threshold
    }

    /// Get number of SSTables in this level
    pub fn num_sstables(&self) -> usize {
        self.sstables.len()
    }

    /// Get current size
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Get level number
    pub fn level_num(&self) -> usize {
        self.level_num
    }

    /// Get SSTables
    pub fn sstables(&self) -> &[PathBuf] {
        &self.sstables
    }
}

/// LSM Tree structure with multiple levels
pub struct LSMTree {
    /// Levels (L0, L1, L2, ...)
    levels: Vec<Level>,
    /// Size ratio between levels (default: 10)
    #[allow(dead_code)]
    size_ratio: u64,
    /// Base level size (L1 threshold)
    #[allow(dead_code)]
    base_size: u64,
    /// Data directory
    data_dir: PathBuf,
}

impl LSMTree {
    /// Create a new LSM tree
    ///
    /// # Arguments
    /// * `data_dir` - Directory for SSTable files
    /// * `base_size` - L1 size threshold (default: 10MB)
    /// * `size_ratio` - Size ratio between levels (default: 10)
    /// * `num_levels` - Number of levels (default: 7)
    pub fn new(
        data_dir: impl AsRef<Path>,
        base_size: u64,
        size_ratio: u64,
        num_levels: usize,
    ) -> Self {
        let mut levels = Vec::with_capacity(num_levels);

        // L0 has special handling (memtable flush target, no size limit)
        levels.push(Level::new(0, u64::MAX));

        // L1+ have exponentially increasing thresholds
        for i in 1..num_levels {
            let threshold = base_size * size_ratio.pow((i - 1) as u32);
            levels.push(Level::new(i, threshold));
        }

        Self {
            levels,
            size_ratio,
            base_size,
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Add an SSTable to L0 (memtable flush)
    pub fn add_l0_sstable(&mut self, path: PathBuf, size: u64) {
        self.levels[0].add_sstable(path, size);
    }

    /// Check if any level needs compaction
    pub fn needs_compaction(&self) -> Option<usize> {
        // Check L0 first (special case: trigger on # of files, not size)
        if self.levels[0].num_sstables() >= 4 {
            return Some(0);
        }

        // Check other levels by size
        for (i, level) in self.levels.iter().enumerate().skip(1) {
            if level.needs_compaction() {
                return Some(i);
            }
        }

        None
    }

    /// Get a level
    pub fn level(&self, level_num: usize) -> Option<&Level> {
        self.levels.get(level_num)
    }

    /// Get number of levels
    pub fn num_levels(&self) -> usize {
        self.levels.len()
    }

    /// Get data directory
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Add an SSTable to a specific level (after compaction)
    pub fn add_to_level(&mut self, level_num: usize, path: PathBuf, size: u64) {
        if let Some(level) = self.levels.get_mut(level_num) {
            level.add_sstable(path, size);
        }
    }

    /// Remove specific SSTables from a level (after compaction)
    pub fn remove_sstables_from_level(&mut self, level_num: usize, paths: &[PathBuf]) {
        if let Some(level) = self.levels.get_mut(level_num) {
            for path in paths {
                if let Some(pos) = level.sstables.iter().position(|p| p == path) {
                    let removed_path = level.sstables.remove(pos);

                    // Update size (need to get file size)
                    if let Ok(metadata) = std::fs::metadata(&removed_path) {
                        let size = metadata.len();
                        level.size = level.size.saturating_sub(size);
                    }
                }
            }
        }
    }

    /// Clear all SSTables from a level (used during compaction)
    pub fn clear_level(&mut self, level_num: usize) -> Vec<PathBuf> {
        if let Some(level) = self.levels.get_mut(level_num) {
            let paths = std::mem::take(&mut level.sstables);
            level.size = 0;
            paths
        } else {
            Vec::new()
        }
    }

    /// Load existing SSTables from disk into the LSM tree
    ///
    /// Scans the data directory for SSTable files and adds them to L0.
    /// This is called during DB::open() to recover existing data.
    pub fn load_existing_sstables(&mut self) -> std::io::Result<()> {
        use crate::sstable::SSTable;

        // Scan data directory for .sst files
        let entries = std::fs::read_dir(&self.data_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Only process .sst files
            if path.extension().and_then(|s| s.to_str()) == Some("sst") {
                // Get file size
                let metadata = std::fs::metadata(&path)?;
                let size = metadata.len();

                // Verify SSTable by opening it and validating all blocks
                // If corrupt, this will return an error
                let mut sstable = SSTable::open(&path).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Corrupt SSTable: {}", e),
                    )
                })?;

                // Validate all blocks to detect corruption
                sstable.validate().map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Corrupt SSTable: {}", e),
                    )
                })?;

                // Add to L0 (all recovered SSTables go to L0)
                self.levels[0].add_sstable(path, size);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tempfile::tempdir;

    #[test]
    fn test_level_creation() {
        let level = Level::new(1, 10_000_000);
        assert_eq!(level.level_num(), 1);
        assert_eq!(level.size(), 0);
        assert!(!level.needs_compaction());
    }

    #[test]
    fn test_level_compaction_trigger() {
        let mut level = Level::new(1, 1000);
        assert!(!level.needs_compaction());

        level.add_sstable(PathBuf::from("test.sst"), 500);
        assert!(!level.needs_compaction());

        level.add_sstable(PathBuf::from("test2.sst"), 600);
        assert!(level.needs_compaction());
    }

    #[test]
    fn test_lsm_tree_creation() {
        let dir = tempdir().unwrap();
        let lsm = LSMTree::new(dir.path(), 10_000_000, 10, 7);

        assert_eq!(lsm.num_levels(), 7);
        assert_eq!(lsm.level(0).unwrap().level_num(), 0);
        assert_eq!(lsm.level(1).unwrap().size_threshold, 10_000_000);
        assert_eq!(lsm.level(2).unwrap().size_threshold, 100_000_000);
    }

    #[test]
    fn test_l0_compaction_trigger() {
        let dir = tempdir().unwrap();
        let mut lsm = LSMTree::new(dir.path(), 10_000_000, 10, 7);

        assert!(lsm.needs_compaction().is_none());

        // Add 4 SSTables to L0 (triggers compaction)
        for i in 0..4 {
            lsm.add_l0_sstable(PathBuf::from(format!("test{}.sst", i)), 1000);
        }

        assert_eq!(lsm.needs_compaction(), Some(0));
    }

    #[test]
    fn test_level_size_compaction_trigger() {
        let dir = tempdir().unwrap();
        let mut lsm = LSMTree::new(dir.path(), 1000, 10, 7);

        // Add enough data to L1 to trigger compaction
        lsm.levels[1].add_sstable(PathBuf::from("test.sst"), 1200);

        assert_eq!(lsm.needs_compaction(), Some(1));
    }

    #[test]
    fn test_compact_sstables() {
        use crate::sstable::SSTableBuilder;

        let dir = tempdir().unwrap();

        // Build first SSTable
        let path1 = dir.path().join("input1.sst");
        let mut builder1 = SSTableBuilder::create(&path1).unwrap();
        builder1.add(Bytes::from("key1"), Bytes::from("value1")).unwrap();
        builder1.add(Bytes::from("key3"), Bytes::from("value3")).unwrap();
        builder1.finish().unwrap();

        // Build second SSTable
        let path2 = dir.path().join("input2.sst");
        let mut builder2 = SSTableBuilder::create(&path2).unwrap();
        builder2.add(Bytes::from("key2"), Bytes::from("value2")).unwrap();
        builder2.add(Bytes::from("key4"), Bytes::from("value4")).unwrap();
        builder2.finish().unwrap();

        // Compact
        let output_path = dir.path().join("output.sst");
        let (result_path, size) = compact_sstables(&[path1, path2], &output_path).unwrap();

        assert_eq!(result_path, output_path);
        assert!(size > 0);

        // Verify output SSTable has merged data
        let mut output_sst = SSTable::open(&output_path).unwrap();
        assert_eq!(output_sst.len(), 4);

        assert_eq!(
            output_sst.get(b"key1").unwrap(),
            Some(Bytes::from("value1"))
        );
        assert_eq!(
            output_sst.get(b"key2").unwrap(),
            Some(Bytes::from("value2"))
        );
        assert_eq!(
            output_sst.get(b"key3").unwrap(),
            Some(Bytes::from("value3"))
        );
        assert_eq!(
            output_sst.get(b"key4").unwrap(),
            Some(Bytes::from("value4"))
        );
    }

    #[test]
    fn test_compact_with_duplicates() {
        use crate::sstable::SSTableBuilder;

        let dir = tempdir().unwrap();

        // Build newer SSTable
        let path1 = dir.path().join("input1.sst");
        let mut builder1 = SSTableBuilder::create(&path1).unwrap();
        builder1.add(Bytes::from("key1"), Bytes::from("new_value")).unwrap();
        builder1.finish().unwrap();

        // Build older SSTable
        let path2 = dir.path().join("input2.sst");
        let mut builder2 = SSTableBuilder::create(&path2).unwrap();
        builder2.add(Bytes::from("key1"), Bytes::from("old_value")).unwrap();
        builder2.add(Bytes::from("key2"), Bytes::from("value2")).unwrap();
        builder2.finish().unwrap();

        // Compact (newer first)
        let output_path = dir.path().join("output.sst");
        compact_sstables(&[path1, path2], &output_path).unwrap();

        // Verify output has newer value
        let mut output_sst = SSTable::open(&output_path).unwrap();
        assert_eq!(output_sst.len(), 2);

        assert_eq!(
            output_sst.get(b"key1").unwrap(),
            Some(Bytes::from("new_value"))
        );
        assert_eq!(
            output_sst.get(b"key2").unwrap(),
            Some(Bytes::from("value2"))
        );
    }
}

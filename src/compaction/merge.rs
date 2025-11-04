// Merge iterator for compaction
// Merges multiple sorted SSTables

use crate::sstable::{SSTable, SSTableError};
use bytes::Bytes;

/// Merged entries from all SSTables, sorted by key
pub struct MergeIterator {
    /// All entries from all SSTables, sorted and deduplicated
    entries: Vec<(Bytes, Bytes)>,
    /// Current position
    position: usize,
}

impl MergeIterator {
    /// Create a new merge iterator from multiple SSTables
    ///
    /// Collects all entries, sorts by key, and deduplicates (keeps newest).
    /// For compaction, "newest" means from lower source_id (earlier in vector).
    pub fn new(mut sstables: Vec<SSTable>) -> Result<Self, SSTableError> {
        let mut all_entries = Vec::new();

        // Collect all entries from all SSTables
        for (source_id, sstable) in sstables.iter_mut().enumerate() {
            let iter = sstable.iter()?;

            for result in iter {
                let (key, value) = result?;
                all_entries.push((key, value, source_id));
            }
        }

        // Sort by key first, then by source_id (lower = newer)
        all_entries.sort_by(|a, b| {
            match a.0.cmp(&b.0) {
                std::cmp::Ordering::Equal => a.2.cmp(&b.2), // Lower source_id first
                other => other,
            }
        });

        // Deduplicate: keep first occurrence of each key (lowest source_id = newest)
        let mut deduplicated = Vec::new();
        let mut last_key: Option<Bytes> = None;

        for (key, value, _source_id) in all_entries {
            if let Some(ref last) = last_key {
                if key == last {
                    continue; // Duplicate, skip
                }
            }

            deduplicated.push((key.clone(), value));
            last_key = Some(key);
        }

        Ok(Self {
            entries: deduplicated,
            position: 0,
        })
    }

    /// Check if iterator is exhausted
    pub fn is_empty(&self) -> bool {
        self.position >= self.entries.len()
    }

    /// Get total number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Iterator implementation for MergeIterator
impl Iterator for MergeIterator {
    type Item = Result<(Bytes, Bytes), SSTableError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.entries.len() {
            return None;
        }

        let entry = self.entries[self.position].clone();
        self.position += 1;
        Some(Ok(entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sstable::SSTableBuilder;
    use tempfile::tempdir;

    #[test]
    fn test_merge_single_sstable() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sst");

        // Build single SSTable
        let mut builder = SSTableBuilder::create(&path).unwrap();
        builder.add(Bytes::from("key1"), Bytes::from("value1")).unwrap();
        builder.add(Bytes::from("key2"), Bytes::from("value2")).unwrap();
        builder.add(Bytes::from("key3"), Bytes::from("value3")).unwrap();
        builder.finish().unwrap();
        let sstable = SSTable::open(&path).unwrap();

        // Merge (single iterator)
        let mut merge = MergeIterator::new(vec![sstable]).unwrap();

        let (k, v) = merge.next().unwrap().unwrap();
        assert_eq!(k, Bytes::from("key1"));
        assert_eq!(v, Bytes::from("value1"));

        let (k, v) = merge.next().unwrap().unwrap();
        assert_eq!(k, Bytes::from("key2"));
        assert_eq!(v, Bytes::from("value2"));

        let (k, v) = merge.next().unwrap().unwrap();
        assert_eq!(k, Bytes::from("key3"));
        assert_eq!(v, Bytes::from("value3"));

        assert!(merge.next().is_none());
    }

    #[test]
    fn test_merge_two_sstables() {
        let dir = tempdir().unwrap();

        // Build first SSTable
        let path1 = dir.path().join("test1.sst");
        let mut builder1 = SSTableBuilder::create(&path1).unwrap();
        builder1.add(Bytes::from("key1"), Bytes::from("value1")).unwrap();
        builder1.add(Bytes::from("key3"), Bytes::from("value3")).unwrap();
        builder1.finish().unwrap();
        let sstable1 = SSTable::open(&path1).unwrap();

        // Build second SSTable
        let path2 = dir.path().join("test2.sst");
        let mut builder2 = SSTableBuilder::create(&path2).unwrap();
        builder2.add(Bytes::from("key2"), Bytes::from("value2")).unwrap();
        builder2.add(Bytes::from("key4"), Bytes::from("value4")).unwrap();
        builder2.finish().unwrap();
        let sstable2 = SSTable::open(&path2).unwrap();

        // Merge
        let mut merge = MergeIterator::new(vec![sstable1, sstable2]).unwrap();

        let entries: Vec<_> = std::iter::from_fn(|| merge.next())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].0, Bytes::from("key1"));
        assert_eq!(entries[1].0, Bytes::from("key2"));
        assert_eq!(entries[2].0, Bytes::from("key3"));
        assert_eq!(entries[3].0, Bytes::from("key4"));
    }

    #[test]
    fn test_merge_with_duplicates() {
        let dir = tempdir().unwrap();

        // Build first SSTable (newer)
        let path1 = dir.path().join("test1.sst");
        let mut builder1 = SSTableBuilder::create(&path1).unwrap();
        builder1.add(Bytes::from("key1"), Bytes::from("new_value1")).unwrap();
        builder1.add(Bytes::from("key2"), Bytes::from("new_value2")).unwrap();
        builder1.finish().unwrap();
        let sstable1 = SSTable::open(&path1).unwrap();

        // Build second SSTable (older)
        let path2 = dir.path().join("test2.sst");
        let mut builder2 = SSTableBuilder::create(&path2).unwrap();
        builder2.add(Bytes::from("key1"), Bytes::from("old_value1")).unwrap();
        builder2.add(Bytes::from("key3"), Bytes::from("value3")).unwrap();
        builder2.finish().unwrap();
        let sstable2 = SSTable::open(&path2).unwrap();

        // Merge (newer first)
        let mut merge = MergeIterator::new(vec![sstable1, sstable2]).unwrap();

        let entries: Vec<_> = std::iter::from_fn(|| merge.next())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, Bytes::from("key1"));
        assert_eq!(entries[0].1, Bytes::from("new_value1")); // Keeps newer
        assert_eq!(entries[1].0, Bytes::from("key2"));
        assert_eq!(entries[2].0, Bytes::from("key3"));
    }

    #[test]
    fn test_merge_many_sstables() {
        let dir = tempdir().unwrap();
        let mut sstables = Vec::new();

        // Build 5 SSTables with interleaved keys
        for i in 0..5 {
            let path = dir.path().join(format!("test{}.sst", i));
            let mut builder = SSTableBuilder::create(&path).unwrap();

            for j in 0..10 {
                let key = format!("key_{:03}", i + j * 5);
                let value = format!("value_{}", i + j * 5);
                builder.add(Bytes::from(key), Bytes::from(value)).unwrap();
            }

            builder.finish().unwrap();
            let sstable = SSTable::open(&path).unwrap();
            sstables.push(sstable);
        }

        // Merge all
        let mut merge = MergeIterator::new(sstables).unwrap();

        let mut count = 0;
        let mut last_key = None;

        while let Some(Ok((key, _value))) = merge.next() {
            // Verify sorted order
            if let Some(ref last) = last_key {
                assert!(key > *last, "Keys not in sorted order");
            }
            last_key = Some(key);
            count += 1;
        }

        assert_eq!(count, 50); // 5 SSTables * 10 keys each
    }
}

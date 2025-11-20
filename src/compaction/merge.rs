// Merge iterator for compaction
// Merges multiple sorted SSTables

use crate::compaction::{CompactionFilter, FilterDecision};
use crate::sstable::{SSTable, SSTableError};
use bytes::Bytes;
use std::sync::Arc;

/// Merged entries from all SSTables, sorted by key
pub struct MergeIterator {
    entries: std::vec::IntoIter<(Bytes, Bytes)>,
}

impl MergeIterator {
    /// Create a new merge iterator from multiple SSTables
    ///
    /// Collects all entries, sorts by key, and deduplicates (keeps newest).
    /// For L0 compaction, "newest" means from HIGHER source_id (later in vector).
    /// L0 SSTables are ordered oldest→newest, so higher index = newer data.
    pub fn new(
        mut sstables: Vec<SSTable>,
        level: usize,
        filter: Option<Arc<dyn CompactionFilter>>,
    ) -> Result<Self, SSTableError> {
        let mut all_entries = Vec::new();

        // Collect all entries from all SSTables
        for (source_id, sstable) in sstables.iter_mut().enumerate() {
            let iter = sstable.iter()?;

            for result in iter {
                let (key, value) = result?;
                all_entries.push((key, value, source_id));
            }
        }

        // Sort by key first, then by source_id DESCENDING (higher = newer)
        // CRITICAL FIX: Higher source_id means newer SSTable in L0
        all_entries.sort_by(|a, b| {
            match a.0.cmp(&b.0) {
                std::cmp::Ordering::Equal => b.2.cmp(&a.2), // Higher source_id first (NEWEST)
                other => other,
            }
        });

        let mut finalized_entries = Vec::new();

        if let Some(filter) = filter {
            // Group by key and apply filter logic
            let mut i = 0;
            while i < all_entries.len() {
                let key = &all_entries[i].0;
                let mut j = i + 1;
                
                // Find all versions of this key
                while j < all_entries.len() && all_entries[j].0 == key {
                    j += 1;
                }
                
                // Slice of all versions for this key (already sorted by recency)
                let versions = &all_entries[i..j];
                
                // 1. Merge phase
                let values: Vec<&[u8]> = versions.iter().map(|(_, v, _)| v.as_ref()).collect();
                
                // Default: pick newest (first in slice)
                let newest_value = &versions[0].1;
                let mut merged_value_bytes = newest_value.clone();
                
                // Try custom merge
                if let Some(merged) = filter.merge(level, key, &values) {
                    // If merged, we treat it as a new INLINE value
                    // Prepend FLAG_INLINE (1)
                    let mut with_flag = Vec::with_capacity(1 + merged.len());
                    with_flag.push(crate::sstable::FLAG_INLINE);
                    with_flag.extend_from_slice(&merged);
                    merged_value_bytes = Bytes::from(with_flag);
                }

                // 2. Filter phase
                match filter.filter(level, key, &merged_value_bytes) {
                    FilterDecision::Keep => {
                        finalized_entries.push((key.clone(), merged_value_bytes));
                    }
                    FilterDecision::Remove => {
                        // Skip
                    }
                    FilterDecision::ChangeValue(new_val) => {
                        // Treat as new INLINE value
                        let mut with_flag = Vec::with_capacity(1 + new_val.len());
                        with_flag.push(crate::sstable::FLAG_INLINE);
                        with_flag.extend_from_slice(&new_val);
                        finalized_entries.push((key.clone(), Bytes::from(with_flag)));
                    }
                }

                // Advance to next key
                i = j;
            }
        } else {
            // Fast path: Simple deduplication (keep newest)
            let mut last_key: Option<Bytes> = None;

            for (key, value, _source_id) in all_entries {
                if let Some(ref last) = last_key {
                    if key == last {
                        continue; // Duplicate, skip
                    }
                }

                finalized_entries.push((key.clone(), value));
                last_key = Some(key);
            }
        }

        Ok(Self {
            entries: finalized_entries.into_iter(),
        })
    }
}

impl Iterator for MergeIterator {
    type Item = Result<(Bytes, Bytes), SSTableError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(Ok)
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
        builder
            .add(Bytes::from("key1"), Bytes::from("value1"))
            .unwrap();
        builder
            .add(Bytes::from("key2"), Bytes::from("value2"))
            .unwrap();
        builder
            .add(Bytes::from("key3"), Bytes::from("value3"))
            .unwrap();
        builder.finish().unwrap();
        let sstable = SSTable::open(&path).unwrap();

        // Merge (single iterator)
        let mut merge = MergeIterator::new(vec![sstable], 0, None).unwrap();

        let (k, v) = merge.next().unwrap().unwrap();
        assert_eq!(k, Bytes::from("key1"));
        // MergeIterator returns FLAG-prefixed values (for compaction)
        assert_eq!(v[0], crate::sstable::FLAG_INLINE);
        assert_eq!(&v[1..], b"value1");

        let (k, v) = merge.next().unwrap().unwrap();
        assert_eq!(k, Bytes::from("key2"));
        assert_eq!(v[0], crate::sstable::FLAG_INLINE);
        assert_eq!(&v[1..], b"value2");

        let (k, v) = merge.next().unwrap().unwrap();
        assert_eq!(k, Bytes::from("key3"));
        assert_eq!(v[0], crate::sstable::FLAG_INLINE);
        assert_eq!(&v[1..], b"value3");

        assert!(merge.next().is_none());
    }

    #[test]
    fn test_merge_two_sstables() {
        let dir = tempdir().unwrap();

        // Build first SSTable
        let path1 = dir.path().join("test1.sst");
        let mut builder1 = SSTableBuilder::create(&path1).unwrap();
        builder1
            .add(Bytes::from("key1"), Bytes::from("value1"))
            .unwrap();
        builder1
            .add(Bytes::from("key3"), Bytes::from("value3"))
            .unwrap();
        builder1.finish().unwrap();
        let sstable1 = SSTable::open(&path1).unwrap();

        // Build second SSTable
        let path2 = dir.path().join("test2.sst");
        let mut builder2 = SSTableBuilder::create(&path2).unwrap();
        builder2
            .add(Bytes::from("key2"), Bytes::from("value2"))
            .unwrap();
        builder2
            .add(Bytes::from("key4"), Bytes::from("value4"))
            .unwrap();
        builder2.finish().unwrap();
        let sstable2 = SSTable::open(&path2).unwrap();

        // Merge
        let mut merge = MergeIterator::new(vec![sstable1, sstable2], 0, None).unwrap();

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

        // Build first SSTable (older - like L0 order: older SSTables come first)
        let path1 = dir.path().join("test1.sst");
        let mut builder1 = SSTableBuilder::create(&path1).unwrap();
        builder1
            .add(Bytes::from("key1"), Bytes::from("old_value1"))
            .unwrap();
        builder1
            .add(Bytes::from("key3"), Bytes::from("value3"))
            .unwrap();
        builder1.finish().unwrap();
        let sstable1 = SSTable::open(&path1).unwrap();

        // Build second SSTable (newer - later in L0, higher index)
        let path2 = dir.path().join("test2.sst");
        let mut builder2 = SSTableBuilder::create(&path2).unwrap();
        builder2
            .add(Bytes::from("key1"), Bytes::from("new_value1"))
            .unwrap();
        builder2
            .add(Bytes::from("key2"), Bytes::from("new_value2"))
            .unwrap();
        builder2.finish().unwrap();
        let sstable2 = SSTable::open(&path2).unwrap();

        // Merge (older first, newer second - like L0 order)
        let mut merge = MergeIterator::new(vec![sstable1, sstable2], 0, None).unwrap();

        let entries: Vec<_> = std::iter::from_fn(|| merge.next())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, Bytes::from("key1"));
        // MergeIterator returns FLAG-prefixed values (for compaction)
        assert_eq!(entries[0].1[0], crate::sstable::FLAG_INLINE);
        assert_eq!(&entries[0].1[1..], b"new_value1"); // Keeps newer (higher source_id)
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
        let mut merge = MergeIterator::new(sstables, 0, None).unwrap();

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
    
    // Test compaction filter
    #[derive(Debug)]
    struct TestFilter;
    
    impl CompactionFilter for TestFilter {
        fn filter(&self, _level: usize, key: &[u8], value: &[u8]) -> FilterDecision {
            // Remove key "remove_me"
            if key == b"remove_me" {
                return FilterDecision::Remove;
            }
            
            // Change value for "change_me"
            if key == b"change_me" {
                return FilterDecision::ChangeValue(b"changed".to_vec());
            }
            
            // Check value content (skip flag byte)
            if value.len() > 1 && &value[1..] == b"filter_me" {
                 return FilterDecision::Remove;
            }
            
            FilterDecision::Keep
        }
        
        fn merge(&self, _level: usize, key: &[u8], values: &[&[u8]]) -> Option<Vec<u8>> {
            if key == b"merge_me" {
                // Concat all values (skipping flags)
                let mut merged = Vec::new();
                for v in values {
                    if v.len() > 1 {
                        merged.extend_from_slice(&v[1..]);
                    }
                }
                return Some(merged);
            }
            None
        }
    }
    
    #[test]
    fn test_merge_with_filter() {
        let dir = tempdir().unwrap();
        
        let path1 = dir.path().join("test1.sst");
        let mut builder1 = SSTableBuilder::create(&path1).unwrap();
        builder1.add(Bytes::from("keep_me"), Bytes::from("val1")).unwrap();
        builder1.add(Bytes::from("remove_me"), Bytes::from("val2")).unwrap();
        builder1.add(Bytes::from("change_me"), Bytes::from("val3")).unwrap();
        builder1.add(Bytes::from("filter_by_val"), Bytes::from("filter_me")).unwrap();
        builder1.add(Bytes::from("merge_me"), Bytes::from("part1")).unwrap();
        builder1.finish().unwrap();
        let sstable1 = SSTable::open(&path1).unwrap();
        
        let path2 = dir.path().join("test2.sst");
        let mut builder2 = SSTableBuilder::create(&path2).unwrap();
        builder2.add(Bytes::from("merge_me"), Bytes::from("part2")).unwrap();
        builder2.finish().unwrap();
        let sstable2 = SSTable::open(&path2).unwrap();
        
        let filter = Arc::new(TestFilter);
        let mut merge = MergeIterator::new(vec![sstable1, sstable2], 0, Some(filter)).unwrap();
        
        let entries: Vec<_> = std::iter::from_fn(|| merge.next())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
            
        // "change_me" -> "changed"
        // "filter_by_val" -> Removed
        // "keep_me" -> "val1"
        // "merge_me" -> "part2part1" (newest first, so part2 then part1)
        // "remove_me" -> Removed
        
        assert_eq!(entries.len(), 3);
        
        // change_me
        assert_eq!(entries[0].0, Bytes::from("change_me"));
        assert_eq!(&entries[0].1[1..], b"changed");
        
        // keep_me
        assert_eq!(entries[1].0, Bytes::from("keep_me"));
        assert_eq!(&entries[1].1[1..], b"val1");
        
        // merge_me
        assert_eq!(entries[2].0, Bytes::from("merge_me"));
        // part2 is newer (sstable2), part1 is older (sstable1)
        // merge receives [part2, part1]
        // our merge logic concatenates them -> part2part1
        assert_eq!(&entries[2].1[1..], b"part2part1");
    }
}

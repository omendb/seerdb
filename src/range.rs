// Range scan iterator for efficient key range queries

use crate::memtable::Entry;
use crate::range_merge::KWayMergeIterator;
use crate::sstable::SSTableRangeIterator;
use crate::MergeOperator;
use bytes::Bytes;
use std::sync::Arc;

/// Iterator item: (key, value) pair
pub type RangeItem = (Bytes, Bytes);

/// Adapter to convert SSTable range iterator error types
struct SSTableRangeAdapter<I> {
    inner: I,
}

impl<I> SSTableRangeAdapter<I> {
    fn new(inner: I) -> Self {
        Self { inner }
    }
}

impl<I> Iterator for SSTableRangeAdapter<I>
where
    I: Iterator<Item = crate::sstable::Result<(Bytes, Entry)>>,
{
    type Item = Result<(Bytes, Entry), Box<dyn std::error::Error + Send + Sync>>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|result| result.map_err(Into::into))
    }
}

/// Range iterator that merges memtable and SSTable data using k-way merge
///
/// Provides efficient streaming iteration over key ranges in the database,
/// automatically merging data from memtables and all LSM levels.
pub struct RangeIterator {
    // K-way merge iterator (O(k log k) per entry, O(k) memory)
    inner: KWayMergeIterator<
        Box<dyn Iterator<Item = Result<(Bytes, Entry), Box<dyn std::error::Error + Send + Sync>>>>,
    >,
}

impl RangeIterator {
    /// Create a new range iterator using k-way merge
    ///
    /// # Arguments
    /// * `start_key` - Start of range (inclusive)
    /// * `end_key` - End of range (exclusive), None for open-ended
    /// * `memtables` - Memtable partitions to extract range data from
    /// * `sstable_iters` - Pre-created SSTable range iterators
    /// * `merge_operator` - Optional merge operator for resolving merge entries
    pub fn new(
        start_key: &[u8],
        end_key: Option<&[u8]>,
        memtables: &[&crate::memtable::Memtable],
        sstable_iters: Vec<SSTableRangeIterator>,
        merge_operator: Option<Arc<dyn MergeOperator>>,
    ) -> crate::db::Result<Self> {
        let mut iterators: Vec<
            Box<
                dyn Iterator<
                    Item = Result<(Bytes, Entry), Box<dyn std::error::Error + Send + Sync>>,
                >,
            >,
        > = Vec::new();

        // Level 0: Each memtable partition as a SEPARATE iterator (for proper deduplication)
        for memtable in memtables {
            let partition_entries: Vec<(Bytes, Entry)> = if let Some(end_key) = end_key {
                memtable
                    .range(start_key, end_key)
                    .map(|(key, entry)| (key, entry.clone()))
                    .collect()
            } else {
                memtable
                    .range_from(start_key)
                    .map(|(key, entry)| (key, entry.clone()))
                    .collect()
            };

            let partition_iter: Box<
                dyn Iterator<
                    Item = Result<(Bytes, Entry), Box<dyn std::error::Error + Send + Sync>>,
                >,
            > = Box::new(partition_entries.into_iter().map(Ok));
            iterators.push(partition_iter);
        }

        // Level 1+: SSTable iterators (already created, just adapt them)
        for sst_iter in sstable_iters {
            let adapted: Box<
                dyn Iterator<
                    Item = Result<(Bytes, Entry), Box<dyn std::error::Error + Send + Sync>>,
                >,
            > = Box::new(SSTableRangeAdapter::new(sst_iter));
            iterators.push(adapted);
        }

        // Create k-way merge iterator
        let merge =
            KWayMergeIterator::new(iterators, merge_operator).map_err(std::io::Error::other)?;

        Ok(RangeIterator { inner: merge })
    }
}

impl Iterator for RangeIterator {
    type Item = Result<RangeItem, Box<dyn std::error::Error>>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Convert entries to (key, value)
        let result = self
            .inner
            .next()
            .map(|res| {
                res.map(|(key, entry)| match entry {
                    Entry::Value(val) => (key, val),
                    Entry::Merge(val) => (key, val.last().cloned().unwrap_or_default()), // Return last raw operand if unresolved
                    Entry::Tombstone => (key, Bytes::new()),                             // Should be filtered out
                })
                .map_err(|e| e as Box<dyn std::error::Error>)
            });
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memtable::Memtable;

    #[test]
    fn test_range_iterator_empty() {
        let memtable = Memtable::new(1024 * 1024);
        let memtables = [&memtable];
        let range_iter = RangeIterator::new(b"start", None, &memtables, vec![], None).unwrap();

        assert_eq!(range_iter.count(), 0);
    }

    #[test]
    fn test_range_iterator_memtable_only() {
        let memtable = Memtable::new(1024 * 1024);

        // Insert some test data
        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        memtable.put(Bytes::from("key2"), Bytes::from("value2"));
        memtable.put(Bytes::from("key3"), Bytes::from("value3"));

        let memtables = [&memtable];
        let mut range_iter =
            RangeIterator::new(b"key1", Some(b"key3"), &memtables, vec![], None).unwrap();

        let mut results = vec![];
        while let Some(Ok((key, value))) = range_iter.next() {
            results.push((key, value));
        }

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (Bytes::from("key1"), Bytes::from("value1")));
        assert_eq!(results[1], (Bytes::from("key2"), Bytes::from("value2")));
    }

    #[test]
    fn test_range_iterator_tombstone() {
        let memtable = Memtable::new(1024 * 1024);

        // Insert data and a tombstone
        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        memtable.delete(Bytes::from("key2")); // Tombstone
        memtable.put(Bytes::from("key3"), Bytes::from("value3"));

        let memtables = [&memtable];
        let mut range_iter = RangeIterator::new(b"key1", None, &memtables, vec![], None).unwrap();

        let mut results = vec![];
        while let Some(Ok((key, value))) = range_iter.next() {
            results.push((key, value));
        }

        // Should only return key1 and key3, key2 is deleted
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (Bytes::from("key1"), Bytes::from("value1")));
        assert_eq!(results[1], (Bytes::from("key3"), Bytes::from("value3")));
    }
}

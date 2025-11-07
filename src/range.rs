// Range scan iterator for efficient key range queries

use crate::memtable::Entry;
use crate::range_merge::KWayMergeIterator;
use crate::sstable::SSTableRangeIterator;
use bytes::Bytes;

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
    I: Iterator<Item = crate::sstable::Result<(Bytes, Option<Bytes>)>>,
{
    type Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|result| result.map_err(Into::into))
    }
}

/// Range iterator that merges memtable and SSTable data using k-way merge
///
/// LSM semantics:
/// - Newer entries (memtable, then L0, L1, ... LN) override older entries
/// - Tombstones hide older values
///
/// Truly lazy iteration: Loads blocks on-demand, no upfront materialization
pub struct RangeIterator {
    // K-way merge iterator (O(k log k) per entry, O(k) memory)
    inner: KWayMergeIterator<Box<dyn Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>>>,
}

impl RangeIterator {
    /// Create a new range iterator using k-way merge
    ///
    /// # Arguments
    /// * `start_key` - Start of range (inclusive)
    /// * `end_key` - End of range (exclusive), None for open-ended
    /// * `memtable` - Memtable to extract range data from
    /// * `sstable_iters` - Pre-created SSTable range iterators (in priority order: L0, L1, ..., LN)
    pub fn new(
        start_key: &[u8],
        end_key: Option<&[u8]>,
        memtable: &crate::memtable::Memtable,
        sstable_iters: Vec<SSTableRangeIterator>,
    ) -> crate::db::Result<Self> {
        let mut iterators: Vec<Box<dyn Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>>> = Vec::new();

        // Level 0: Memtable (newest) - collect into Vec since it's behind Mutex
        let memtable_entries: Vec<(Bytes, Option<Bytes>)> = if let Some(end_key) = end_key {
            memtable.range(start_key, end_key)
                .map(|(key, entry)| match entry {
                    Entry::Value(value) => (key, Some(value)),
                    Entry::Tombstone => (key, None),
                })
                .collect()
        } else {
            memtable.range_from(start_key)
                .map(|(key, entry)| match entry {
                    Entry::Value(value) => (key, Some(value)),
                    Entry::Tombstone => (key, None),
                })
                .collect()
        };

        let memtable_iter: Box<dyn Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>> =
            Box::new(memtable_entries.into_iter().map(Ok));
        iterators.push(memtable_iter);

        // Level 1+: SSTable iterators (already created, just adapt them)
        for sst_iter in sstable_iters {
            let adapted: Box<dyn Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>> =
                Box::new(SSTableRangeAdapter::new(sst_iter));
            iterators.push(adapted);
        }

        // Create k-way merge iterator
        let merge = KWayMergeIterator::new(iterators)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        Ok(RangeIterator { inner: merge })
    }
}

impl Iterator for RangeIterator {
    type Item = Result<RangeItem, Box<dyn std::error::Error>>;

    fn next(&mut self) -> Option<Self::Item> {
        // K-way merge already filters tombstones and deduplicates
        // Just unwrap the Option<Bytes> (always Some after tombstone filtering)
        self.inner.next().map(|result| {
            result
                .map(|(key, value_opt)| (key, value_opt.unwrap()))
                .map_err(|e| e as Box<dyn std::error::Error>)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memtable::Memtable;

    #[test]
    fn test_range_iterator_empty() {
        let memtable = Memtable::new(1024 * 1024);
        let range_iter = RangeIterator::new(b"start", None, &memtable, vec![]).unwrap();

        assert_eq!(range_iter.count(), 0);
    }

    #[test]
    fn test_range_iterator_memtable_only() {
        let memtable = Memtable::new(1024 * 1024);

        // Insert some test data
        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        memtable.put(Bytes::from("key2"), Bytes::from("value2"));
        memtable.put(Bytes::from("key3"), Bytes::from("value3"));

        let mut range_iter =
            RangeIterator::new(b"key1", Some(b"key3"), &memtable, vec![]).unwrap();

        let mut results = vec![];
        while let Some(Ok((key, value))) = range_iter.next() {
            results.push((key, value));
        }

        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0],
            (Bytes::from("key1"), Bytes::from("value1"))
        );
        assert_eq!(
            results[1],
            (Bytes::from("key2"), Bytes::from("value2"))
        );
    }

    #[test]
    fn test_range_iterator_tombstone() {
        let memtable = Memtable::new(1024 * 1024);

        // Insert data and a tombstone
        memtable.put(Bytes::from("key1"), Bytes::from("value1"));
        memtable.delete(Bytes::from("key2")); // Tombstone
        memtable.put(Bytes::from("key3"), Bytes::from("value3"));

        let mut range_iter = RangeIterator::new(b"key1", None, &memtable, vec![]).unwrap();

        let mut results = vec![];
        while let Some(Ok((key, value))) = range_iter.next() {
            results.push((key, value));
        }

        // Should only return key1 and key3, key2 is deleted
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0],
            (Bytes::from("key1"), Bytes::from("value1"))
        );
        assert_eq!(
            results[1],
            (Bytes::from("key3"), Bytes::from("value3"))
        );
    }
}

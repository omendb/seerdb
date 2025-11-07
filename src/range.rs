// Range scan iterator for efficient key range queries

use crate::memtable::Entry;
use crate::sstable::SSTable;
use bytes::Bytes;
use std::collections::BTreeMap;

/// Iterator item: (key, value) pair
pub type RangeItem = (Bytes, Bytes);

/// Range iterator that merges memtable and SSTable data
///
/// LSM semantics:
/// - Newer entries (memtable, then L0, L1, ... LN) override older entries
/// - Tombstones hide older values
///
/// Lazy iteration: Only materializes entries on-demand via Iterator::next()
pub struct RangeIterator {
    // Lazy iterator over merged BTreeMap (filters tombstones on-demand)
    merged_iter: std::collections::btree_map::IntoIter<Bytes, Option<Bytes>>,
}

impl RangeIterator {
    /// Create a new range iterator
    ///
    /// # Arguments
    /// * `start_key` - Start of range (inclusive)
    /// * `end_key` - End of range (exclusive), None for open-ended
    /// * `memtable` - Memtable to extract range data from
    /// * `sstables` - SSTables to scan (in priority order: L0, L1, ..., LN)
    pub fn new(
        start_key: &[u8],
        end_key: Option<&[u8]>,
        memtable: &crate::memtable::Memtable,
        mut sstables: Vec<SSTable>,
    ) -> crate::db::Result<Self> {
        // Use BTreeMap for automatic sorting and deduplication
        // Key already in map = newer version, don't override
        let mut merged: BTreeMap<Bytes, Option<Bytes>> = BTreeMap::new();

        // Collect SSTable entries (oldest to newest: LN → L1 → L0)
        // Process in reverse so newer entries override older
        sstables.reverse();
        for sstable in &sstables {
            let sstable_iter = sstable.scan_range(start_key, end_key);
            for result in sstable_iter {
                let (key, value_opt) = result?;
                // Only insert if key not already present (LSM semantics: newer wins)
                merged.entry(key).or_insert(value_opt);
            }
        }

        // Collect memtable entries (newest data)
        let memtable_entries: Vec<_> = if let Some(end_key) = end_key {
            memtable
                .range(start_key, end_key)
                .map(|(key, entry)| match entry {
                    Entry::Value(value) => (key, Some(value)),
                    Entry::Tombstone => (key, None),
                })
                .collect()
        } else {
            memtable
                .range_from(start_key)
                .map(|(key, entry)| match entry {
                    Entry::Value(value) => (key, Some(value)),
                    Entry::Tombstone => (key, None),
                })
                .collect()
        };

        for (key, value_opt) in memtable_entries {
            merged.insert(key, value_opt);
        }

        // Return lazy iterator (doesn't materialize entries until next() is called)
        Ok(RangeIterator {
            merged_iter: merged.into_iter(),
        })
    }
}

impl Iterator for RangeIterator {
    type Item = Result<RangeItem, Box<dyn std::error::Error>>;

    fn next(&mut self) -> Option<Self::Item> {
        // Lazy iteration: skip tombstones, return only live values
        loop {
            match self.merged_iter.next() {
                Some((key, Some(value))) => return Some(Ok((key, value))),
                Some((_, None)) => continue, // Skip tombstone
                None => return None,
            }
        }
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

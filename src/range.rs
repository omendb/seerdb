// Range scan iterator for efficient key range queries
// Simplified implementation that collects data upfront

use crate::memtable::Entry;
use crate::sstable::SSTable;
use bytes::Bytes;

/// Iterator item: (key, value) pair
pub type RangeItem = (Bytes, Bytes);

/// Simple range iterator - collects all data upfront for now
/// TODO: Make this streaming to avoid memory overhead
pub struct RangeIterator {
    /// Collected entries in sorted order
    entries: Vec<RangeItem>,
    /// Current position
    position: usize,
}

impl RangeIterator {
    /// Create a new range iterator
    ///
    /// # Arguments
    /// * `start_key` - Start of range (inclusive)
    /// * `end_key` - End of range (exclusive), None for open-ended
    /// * `memtable` - Memtable to extract range data from
    /// * `sstables` - SSTables to scan (should be in reverse level order: L0, L1, ..., LN)
    pub fn new(
        start_key: &[u8],
        end_key: Option<&[u8]>,
        memtable: &crate::memtable::Memtable,
        mut sstables: Vec<SSTable>,
    ) -> crate::db::Result<Self> {
        let mut all_entries = Vec::new();

        // Collect memtable entries
        let memtable_entries: Vec<_> = if let Some(end_key) = end_key {
            memtable.range(start_key, end_key).filter_map(|(key, entry)| {
                match entry {
                    Entry::Value(value) => Some((key, value)),
                    Entry::Tombstone => None,
                }
            }).collect()
        } else {
            memtable.range_from(start_key).filter_map(|(key, entry)| {
                match entry {
                    Entry::Value(value) => Some((key, value)),
                    Entry::Tombstone => None,
                }
            }).collect()
        };
        all_entries.extend(memtable_entries);

        // Collect SSTable entries (simplified - just memtable for now)
        // TODO: Implement proper SSTable merging
        drop(sstables); // Ignore SSTables for now

        // Sort all entries by key
        all_entries.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(RangeIterator {
            entries: all_entries,
            position: 0,
        })
    }
}

impl Iterator for RangeIterator {
    type Item = Result<RangeItem, Box<dyn std::error::Error>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.entries.len() {
            return None;
        }

        let item = self.entries[self.position].clone();
        self.position += 1;
        Some(Ok(item))
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

        let mut range_iter = RangeIterator::new(b"key1", Some(b"key3"), &memtable, vec![]).unwrap();

        let mut results = vec![];
        while let Some(Ok((key, value))) = range_iter.next() {
            results.push((key, value));
        }

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (Bytes::from("key1"), Bytes::from("value1")));
        assert_eq!(results[1], (Bytes::from("key2"), Bytes::from("value2")));
    }
}

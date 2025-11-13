// K-way merge iterator for range scans
// Merges sorted iterators from memtable + multiple SSTable levels using a min-heap

use crate::simd;
use bytes::Bytes;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

/// Entry in the min-heap for k-way merge
struct HeapEntry<I>
where
    I: Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>,
{
    key: Bytes,
    value: Option<Bytes>,
    level: usize, // Lower = newer (for LSM semantics)
    iter: I,
}

impl<I> Ord for HeapEntry<I>
where
    I: Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>,
{
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: use Reverse wrapper, so normal comparison
        // Primary: by key (ascending) - using SIMD-accelerated comparison!
        // Secondary: by level (ascending, so lower level = newer wins)
        match simd::compare_keys(&self.key, &other.key) {
            Ordering::Equal => self.level.cmp(&other.level),
            ord => ord,
        }
    }
}

impl<I> PartialOrd for HeapEntry<I>
where
    I: Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<I> Eq for HeapEntry<I> where
    I: Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>
{
}

impl<I> PartialEq for HeapEntry<I>
where
    I: Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>,
{
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.level == other.level
    }
}

/// K-way merge iterator
/// Lazily merges sorted iterators from multiple levels using a min-heap
pub struct KWayMergeIterator<I>
where
    I: Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>,
{
    heap: BinaryHeap<Reverse<HeapEntry<I>>>,
    last_key: Option<Bytes>,
}

impl<I> KWayMergeIterator<I>
where
    I: Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>,
{
    /// Create a new k-way merge iterator
    /// Iterators should be provided in order: [memtable, L0, L1, ..., LN]
    /// Level 0 (memtable) is newest, higher levels are older
    pub fn new(iterators: Vec<I>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut heap = BinaryHeap::new();

        // Prime the heap with first entry from each iterator
        for (level, mut iter) in iterators.into_iter().enumerate() {
            if let Some(result) = iter.next() {
                match result {
                    Ok((key, value)) => {
                        heap.push(Reverse(HeapEntry {
                            key,
                            value,
                            level,
                            iter,
                        }));
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        Ok(Self {
            heap,
            last_key: None,
        })
    }
}

impl<I> Iterator for KWayMergeIterator<I>
where
    I: Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>,
{
    type Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Pop smallest key from heap
            let Reverse(mut entry) = self.heap.pop()?;

            // Advance this iterator and push back into heap
            if let Some(result) = entry.iter.next() {
                match result {
                    Ok((next_key, next_value)) => {
                        self.heap.push(Reverse(HeapEntry {
                            key: next_key,
                            value: next_value,
                            level: entry.level,
                            iter: entry.iter,
                        }));
                    }
                    Err(e) => return Some(Err(e)),
                }
            }

            // Deduplicate: skip if same key as last (LSM: first = newest)
            if let Some(ref last) = self.last_key {
                if &entry.key == last {
                    continue; // Duplicate, get next
                }
            }

            self.last_key = Some(entry.key.clone());

            // Skip tombstones (deleted entries)
            if entry.value.is_none() {
                continue;
            }

            return Some(Ok((entry.key, entry.value)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_iter(
        items: Vec<(Bytes, Option<Bytes>)>,
    ) -> impl Iterator<Item = Result<(Bytes, Option<Bytes>), Box<dyn std::error::Error + Send + Sync>>>
    {
        items.into_iter().map(Ok)
    }

    #[test]
    fn test_kway_single_iterator() {
        let iter1 = ok_iter(vec![
            (Bytes::from("a"), Some(Bytes::from("1"))),
            (Bytes::from("b"), Some(Bytes::from("2"))),
            (Bytes::from("c"), Some(Bytes::from("3"))),
        ]);

        let mut merge = KWayMergeIterator::new(vec![iter1]).unwrap();

        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("a"), Some(Bytes::from("1")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("b"), Some(Bytes::from("2")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("c"), Some(Bytes::from("3")))
        );
        assert!(merge.next().is_none());
    }

    #[test]
    fn test_kway_two_iterators() {
        let iter1 = ok_iter(vec![
            (Bytes::from("a"), Some(Bytes::from("1"))),
            (Bytes::from("c"), Some(Bytes::from("3"))),
        ]);

        let iter2 = ok_iter(vec![
            (Bytes::from("b"), Some(Bytes::from("2"))),
            (Bytes::from("d"), Some(Bytes::from("4"))),
        ]);

        let mut merge = KWayMergeIterator::new(vec![iter1, iter2]).unwrap();

        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("a"), Some(Bytes::from("1")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("b"), Some(Bytes::from("2")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("c"), Some(Bytes::from("3")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("d"), Some(Bytes::from("4")))
        );
        assert!(merge.next().is_none());
    }

    #[test]
    fn test_kway_with_duplicates() {
        // Level 0 (newest)
        let iter1 = ok_iter(vec![
            (Bytes::from("a"), Some(Bytes::from("new_a"))),
            (Bytes::from("b"), Some(Bytes::from("new_b"))),
        ]);

        // Level 1 (older)
        let iter2 = ok_iter(vec![
            (Bytes::from("a"), Some(Bytes::from("old_a"))),
            (Bytes::from("c"), Some(Bytes::from("c"))),
        ]);

        let mut merge = KWayMergeIterator::new(vec![iter1, iter2]).unwrap();

        // Should keep newer version (from iter1)
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("a"), Some(Bytes::from("new_a")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("b"), Some(Bytes::from("new_b")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("c"), Some(Bytes::from("c")))
        );
        assert!(merge.next().is_none());
    }

    #[test]
    fn test_kway_with_tombstones() {
        // Level 0 (newest) - key "a" deleted
        let iter1 = ok_iter(vec![
            (Bytes::from("a"), None), // Tombstone
            (Bytes::from("b"), Some(Bytes::from("b"))),
        ]);

        // Level 1 (older) - key "a" exists
        let iter2 = ok_iter(vec![
            (Bytes::from("a"), Some(Bytes::from("old_a"))),
            (Bytes::from("c"), Some(Bytes::from("c"))),
        ]);

        let mut merge = KWayMergeIterator::new(vec![iter1, iter2]).unwrap();

        // Should skip tombstone (deleted key)
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("b"), Some(Bytes::from("b")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("c"), Some(Bytes::from("c")))
        );
        assert!(merge.next().is_none());
    }

    #[test]
    fn test_kway_many_iterators() {
        let iter1 = ok_iter(vec![(Bytes::from("a"), Some(Bytes::from("1")))]);
        let iter2 = ok_iter(vec![(Bytes::from("b"), Some(Bytes::from("2")))]);
        let iter3 = ok_iter(vec![(Bytes::from("c"), Some(Bytes::from("3")))]);
        let iter4 = ok_iter(vec![(Bytes::from("d"), Some(Bytes::from("4")))]);

        let mut merge = KWayMergeIterator::new(vec![iter1, iter2, iter3, iter4]).unwrap();

        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("a"), Some(Bytes::from("1")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("b"), Some(Bytes::from("2")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("c"), Some(Bytes::from("3")))
        );
        assert_eq!(
            merge.next().unwrap().unwrap(),
            (Bytes::from("d"), Some(Bytes::from("4")))
        );
        assert!(merge.next().is_none());
    }

    #[test]
    fn test_kway_empty_iterators() {
        let iter1: Vec<(Bytes, Option<Bytes>)> = vec![];
        let iter2: Vec<(Bytes, Option<Bytes>)> = vec![];

        let mut merge = KWayMergeIterator::new(vec![ok_iter(iter1), ok_iter(iter2)]).unwrap();

        assert!(merge.next().is_none());
    }
}

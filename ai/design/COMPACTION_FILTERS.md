# Compaction Filters Design

**Status**: Implemented
**Priority**: High (Phase 1)
**Required By**: omendb (LSM-VEC)

## Overview

`seerdb` exposes a `CompactionFilter` trait to allow consumers (like `omendb`) to inject custom logic during the compaction process.

## Why it's needed for Vector Search

For **LSM-VEC** (Graph-based LSM), simply compacting SSTables byte-wise is insufficient. When merging two SSTables that contain graph nodes:

1.  **Adjacency List Merging**: If Node A has neighbors `[B, C]` in L0 and `[D, E]` in L1, the compacted version needs `[B, C, D, E]` (or a pruned version), not just the latest key-value pair.
2.  **Garbage Collection**: Deleted vectors need to be removed from the graph structure of neighbors.
3.  **Re-indexing**: Small graph segments might need to be re-optimized during compaction.

## Interface

```rust
pub trait CompactionFilter: Send + Sync + Debug {
    /// Inspect a key-value pair during compaction.
    /// Returns the action to take.
    fn filter(&self, level: usize, key: &[u8], value: &[u8]) -> FilterDecision;
    
    /// Called when merging multiple versions of the same key.
    /// Allows implementing "Merge Operator" logic within compaction.
    ///
    /// # Arguments
    /// * `level` - The output level of the compaction
    /// * `key` - The key being merged
    /// * `values` - List of values for the key, ordered from NEWEST to OLDEST
    ///
    /// # Returns
    /// * `Some(value)` - The merged value to keep (will be passed to `filter` next)
    /// * `None` - If the default behavior (keep newest) should be used
    fn merge(&self, level: usize, key: &[u8], values: &[&[u8]]) -> Option<Vec<u8>>;
}

pub enum FilterDecision {
    Keep,
    Remove,
    ChangeValue(Vec<u8>),
}
```

## Implementation Details

1.  **Defined** the trait in `src/compaction/filter.rs`.
2.  **Added** `compaction_filter: Option<Arc<dyn CompactionFilter>>` to `DBOptions`.
3.  **Integrated** into `MergeIterator` in `src/compaction/merge.rs`.
    *   Groups keys by value.
    *   Calls `merge()` with all versions (if multiple exist).
    *   Calls `filter()` with the resulting value.
    *   Handles `Keep`, `Remove`, `ChangeValue`.
4.  **Wired** through `DB::open`, `spawn_compaction_worker`, and `do_compact_level`.

## Usage Example

```rust
struct MyFilter;
impl CompactionFilter for MyFilter {
    fn filter(&self, _level: usize, key: &[u8], value: &[u8]) -> FilterDecision {
        // Remove keys starting with "tmp"
        if key.starts_with(b"tmp") {
            return FilterDecision::Remove;
        }
        FilterDecision::Keep
    }

    fn merge(&self, _level: usize, _key: &[u8], values: &[&[u8]]) -> Option<Vec<u8>> {
        // Merge counters (sum 64-bit integers)
        let mut sum = 0u64;
        for val in values {
             let bytes: [u8; 8] = val.try_into().ok()?;
             sum += u64::from_le_bytes(bytes);
        }
        Some(sum.to_le_bytes().to_vec())
    }
}
```

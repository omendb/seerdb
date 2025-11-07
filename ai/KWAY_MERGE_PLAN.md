# K-Way Merge Implementation Plan

**Created**: November 6, 2025
**Status**: Ready for implementation
**Priority**: 🔴 CRITICAL (range scans) + ⚠️ Optional (compaction)

---

## Executive Summary

**Problem**: Two locations use eager materialization instead of lazy k-way merge:
1. **Range scans** (src/range.rs) - 20x slower than RocksDB (CRITICAL)
2. **Compaction merge** (src/compaction/merge.rs) - Memory overhead (lower priority)

**Solution**: Implement k-way merge with `BinaryHeap` (proven SOTA approach)

**Expected Impact**:
- Range scans: 870 → 8,000-15,000 scans/sec (**10-20x improvement**)
- Compaction: Lower memory usage, similar throughput

**Research Validation**: ✅ K-way merge is SOTA (confirmed from 2020-2024 papers)
- No newer algorithms found
- Used by: RocksDB, LevelDB, fjall, all production LSMs
- Recent research optimizes orthogonal aspects (filtering, indexing, SIMD)

---

## Problem 1: Range Scans (CRITICAL 🔴)

### Current Implementation (src/range.rs:40-51)

```rust
pub fn scan_range(
    &self,
    start_key: &[u8],
    end_key: Option<&[u8]>,
) -> Result<RangeIterator> {
    let mut merged: BTreeMap<Bytes, Option<Bytes>> = BTreeMap::new();

    // THE PROBLEM: Materializes ALL entries before returning
    for sstable in &sstables {
        let sstable_iter = sstable.scan_range(start_key, end_key);
        for result in sstable_iter {
            let (key, value_opt) = result?;
            merged.entry(key).or_insert(value_opt);  // O(n log n) + O(n) memory
        }
    }

    // Only AFTER loading everything:
    Ok(RangeIterator {
        merged_iter: merged.into_iter(),
    })
}
```

### Performance Impact

**Current**: 870 scans/sec (0.050x RocksDB)
- **fjall**: 10,818 scans/sec → 12x faster
- **RocksDB**: 17,332 scans/sec → 20x faster
- **sled**: 40,948 scans/sec → 47x faster (B-tree advantage)

**Root Cause**:
- **Complexity**: O(n log n) insertion + O(n) memory upfront
- **Latency**: Must load ALL entries before returning first result
- **Memory**: Holds all entries in RAM simultaneously

For 100K entry scan across 7 levels:
- **Current**: Load all 100K → insert into BTreeMap → THEN start returning
- **Correct**: Return first entry immediately, load blocks on-demand

### Target Performance

**Expected**: 8,000-15,000 scans/sec (0.5-0.9x RocksDB)
- **10-20x improvement**
- Makes seerdb viable for general-purpose use
- Unblocks range-heavy workloads

---

## Problem 2: Compaction Merge (Lower Priority ⚠️)

### Current Implementation (src/compaction/merge.rs:18-36)

```rust
pub fn new(mut sstables: Vec<SSTable>) -> Result<Self, SSTableError> {
    let mut all_entries = Vec::new();

    // THE PROBLEM: Materializes ALL entries from ALL SSTables
    for (source_id, sstable) in sstables.iter_mut().enumerate() {
        let iter = sstable.iter()?;

        for result in iter {
            let (key, value) = result?;
            all_entries.push((key, value, source_id));  // Collects everything
        }
    }

    // O(n log n) sort
    all_entries.sort_by(|a, b| {
        match a.0.cmp(&b.0) {
            std::cmp::Ordering::Equal => a.2.cmp(&b.2),
            other => other,
        }
    });

    // Deduplicate
    let mut deduplicated = Vec::new();
    // ...
}
```

### Performance Impact

**Current**: Unmeasured (background operation)
- Memory overhead proportional to compaction size
- Acceptable for small/medium compactions
- Could cause OOM for large compactions (100MB+ SSTables)

**Complexity**:
- **Time**: O(n log n) sort
- **Memory**: O(n) - all entries in RAM

**Priority**: LOW (not on critical path, acceptable for now)

---

## Solution: K-Way Merge with BinaryHeap

### Algorithm

**Standard LSM approach** (used by RocksDB, fjall, all production systems):

```rust
struct HeapEntry {
    key: Bytes,
    value: Option<Bytes>,
    level: usize,  // For LSM semantics (lower = newer)
    iter: Box<dyn Iterator<Item = Result<(Bytes, Option<Bytes>)>>>,
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap by key, break ties with level (lower = newer = higher priority)
        match other.key.cmp(&self.key) {  // Reverse for min-heap
            Ordering::Equal => other.level.cmp(&self.level),  // Lower level wins
            ord => ord,
        }
    }
}

pub struct KWayMergeIterator {
    heap: BinaryHeap<Reverse<HeapEntry>>,
    last_key: Option<Bytes>,  // For deduplication
}

impl Iterator for KWayMergeIterator {
    type Item = Result<(Bytes, Option<Bytes>)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // 1. Pop smallest key from heap
            let Reverse(mut entry) = self.heap.pop()?;

            // 2. Advance this iterator and push back into heap
            if let Some(Ok((next_key, next_value))) = entry.iter.next() {
                self.heap.push(Reverse(HeapEntry {
                    key: next_key,
                    value: next_value,
                    level: entry.level,
                    iter: entry.iter,
                }));
            }

            // 3. Deduplicate: skip if same key as last (LSM: first = newest)
            if let Some(ref last) = self.last_key {
                if &entry.key == last {
                    continue;  // Duplicate, get next
                }
            }

            self.last_key = Some(entry.key.clone());

            // 4. Skip tombstones (for range scans)
            if entry.value.is_none() {
                continue;
            }

            return Some(Ok((entry.key, entry.value)));
        }
    }
}
```

### Complexity Analysis

**K-way merge**:
- **Time**: O(k log k) per entry (k = num levels, typically 7-10)
- **Memory**: O(k) - only heap state
- **Latency**: First result immediate

**vs Current (BTreeMap)**:
- **Time**: O(n log n) upfront
- **Memory**: O(n) - all entries
- **Latency**: After loading everything

**For 100K entries across 7 levels**:
- K-way: O(100K * log 7) ≈ 280K operations, O(7) memory
- BTreeMap: O(100K * log 100K) ≈ 1.66M operations, O(100K) memory

**Expected speedup**: 6x from complexity + lazy loading = **10-20x total**

---

## Implementation Plan

### Phase 1: Range Scans (CRITICAL - 3-4 hours)

#### Step 1: Create `src/range_merge.rs` (1.5 hours)

**File structure**:
```rust
// src/range_merge.rs

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use bytes::Bytes;
use crate::sstable::SSTableRangeIterator;
use crate::memtable::MemTable;

/// Wrapper for heap ordering
struct HeapEntry {
    key: Bytes,
    value: Option<Bytes>,
    level: usize,
    iter: Box<dyn Iterator<Item = Result<(Bytes, Option<Bytes>)>>>,
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap by key, break ties with level (lower = newer)
        match other.key.cmp(&self.key) {
            Ordering::Equal => other.level.cmp(&self.level),
            ord => ord,
        }
    }
}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for HeapEntry {}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.level == other.level
    }
}

/// K-way merge iterator for range scans
pub struct KWayMergeIterator {
    heap: BinaryHeap<Reverse<HeapEntry>>,
    last_key: Option<Bytes>,
}

impl KWayMergeIterator {
    pub fn new(
        memtable: &MemTable,
        sstables: &[Arc<Mutex<SSTable>>],
        start_key: &[u8],
        end_key: Option<&[u8]>,
    ) -> Result<Self> {
        let mut heap = BinaryHeap::new();

        // Level 0: Memtable (newest)
        let memtable_iter = memtable.range(start_key, end_key);
        if let Some(Ok((key, value))) = memtable_iter.next() {
            heap.push(Reverse(HeapEntry {
                key,
                value: Some(value),
                level: 0,
                iter: Box::new(memtable_iter),
            }));
        }

        // Level 1+: SSTables (oldest to newest, but level controls priority)
        for (idx, sstable) in sstables.iter().enumerate() {
            let mut sst = sstable.lock().unwrap();
            let mut sst_iter = sst.scan_range(start_key, end_key);

            if let Some(Ok((key, value_opt))) = sst_iter.next() {
                heap.push(Reverse(HeapEntry {
                    key,
                    value: value_opt,
                    level: idx + 1,  // Higher level = older
                    iter: Box::new(sst_iter),
                }));
            }
        }

        Ok(Self {
            heap,
            last_key: None,
        })
    }
}

impl Iterator for KWayMergeIterator {
    type Item = Result<(Bytes, Option<Bytes>)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Reverse(mut entry) = self.heap.pop()?;

            // Advance this iterator
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
                    Err(e) => return Some(Err(e.into())),
                }
            }

            // Deduplicate
            if let Some(ref last) = self.last_key {
                if &entry.key == last {
                    continue;
                }
            }

            self.last_key = Some(entry.key.clone());

            // Skip tombstones
            if entry.value.is_none() {
                continue;
            }

            return Some(Ok((entry.key, entry.value)));
        }
    }
}
```

**Key challenges**:
- **Trait object lifetimes**: Need `Box<dyn Iterator>` to avoid lifetime issues
- **Error handling**: Propagate errors from inner iterators
- **Ordering**: Ensure min-heap with correct LSM semantics

#### Step 2: Update `src/range.rs` (30 minutes)

```rust
// src/range.rs

use crate::range_merge::KWayMergeIterator;

impl DB {
    pub fn scan_range(
        &self,
        start_key: &[u8],
        end_key: Option<&[u8]>,
    ) -> Result<RangeIterator> {
        let state = self.state.read().unwrap();

        // K-way merge replaces BTreeMap materialization
        let iter = KWayMergeIterator::new(
            &state.memtable,
            &state.sstables,
            start_key,
            end_key,
        )?;

        Ok(RangeIterator { inner: iter })
    }
}

pub struct RangeIterator {
    inner: KWayMergeIterator,
}

impl Iterator for RangeIterator {
    type Item = Result<(Bytes, Bytes)>;

    fn next(&mut self) -> Option<Self::Item> {
        // Filter out tombstones (already done in KWayMergeIterator)
        self.inner.next().map(|r| r.map(|(k, v)| (k, v.unwrap())))
    }
}
```

#### Step 3: Update module declarations (10 minutes)

```rust
// src/lib.rs
mod range_merge;  // Add this
```

#### Step 4: Run tests (30 minutes)

```bash
cargo test --release --lib range
```

**Tests to verify**:
- `test_range_iterator_memtable_only`
- `test_range_iterator_tombstone`
- `test_range_iterator_empty`
- `db::tests::test_range_scan_with_sstables`
- `db::tests::test_range_scan_with_deletes`
- `db::tests::test_range_scan_with_overwrites`

#### Step 5: Benchmark (30 minutes)

```bash
cargo run --example baseline_benchmark --release --features baseline-benchmarks
```

**Expected results**:
- Range scans: 870 → 8,000-15,000 scans/sec
- Reads/writes: Unchanged (not affected)

---

### Phase 2: Compaction Merge (OPTIONAL - 2-3 hours)

**Priority**: LOW (not on critical path)

#### Step 1: Create `src/compaction/kway_merge.rs`

Similar structure to range_merge.rs, but:
- No tombstone filtering (compaction needs them)
- Returns raw FLAG-prefixed values (for SSTable writing)
- Simpler (no memtable, just SSTables)

```rust
pub struct CompactionMergeIterator {
    heap: BinaryHeap<Reverse<HeapEntry>>,
    last_key: Option<Bytes>,
}

impl CompactionMergeIterator {
    pub fn new(sstables: Vec<SSTable>) -> Result<Self> {
        // Similar to KWayMergeIterator but for compaction
        // ...
    }
}

impl Iterator for CompactionMergeIterator {
    type Item = Result<(Bytes, Bytes)>;  // Raw values with FLAG prefix

    fn next(&mut self) -> Option<Self::Item> {
        // Keep tombstones (compaction needs them)
        // ...
    }
}
```

#### Step 2: Update `src/compaction/merge.rs`

Replace eager materialization with k-way merge iterator.

#### Step 3: Run compaction tests

```bash
cargo test --release compaction
```

---

## Testing Strategy

### Unit Tests

**Range scans** (src/range.rs):
- [x] Existing: memtable-only, tombstones, empty
- [ ] New: Large scans (10K+ entries)
- [ ] New: Partial scans (early termination)
- [ ] New: Multiple levels overlap

**Compaction** (src/compaction/merge.rs):
- [x] Existing: Single SSTable, two SSTables, duplicates, many SSTables
- [ ] New: Large compactions (100K+ entries)
- [ ] New: Memory usage validation

### Integration Tests

**Database tests** (src/db.rs):
- [x] Existing: range_scan_with_sstables, range_scan_with_deletes, range_scan_with_overwrites
- [ ] New: Range scan stress test (1M entries)

### Performance Tests

**Baseline benchmark**:
```bash
cargo run --example baseline_benchmark --release --features baseline-benchmarks
```

**Expected**:
- Range scans: 870 → 8,000-15,000 scans/sec (10-20x)
- No regression in other workloads

---

## Error Handling

### Potential Issues

**1. Trait object lifetimes**:
- **Issue**: `Box<dyn Iterator>` requires `'static` or careful lifetime management
- **Solution**: Use `Box<dyn Iterator + 'a>` with appropriate lifetime bounds

**2. Error propagation**:
- **Issue**: Inner iterator errors need to bubble up
- **Solution**: Return `Result<(Bytes, Option<Bytes>)>` and handle in loop

**3. Empty iterators**:
- **Issue**: SSTable might have no entries in range
- **Solution**: Check first entry before adding to heap

**4. Duplicate keys across levels**:
- **Issue**: Must keep newest entry only
- **Solution**: Track `last_key` and skip duplicates (level ordering ensures newest first)

---

## Performance Validation

### Metrics to Track

**Range scans**:
- Throughput: scans/sec (target: 8,000-15,000)
- Latency: ms/scan (target: <0.1ms for 100-key scans)
- Memory: Peak usage (should be O(k), not O(n))

**Compaction**:
- Throughput: entries/sec (should be similar or better)
- Memory: Peak usage (should be lower)
- Time: Total compaction time (should be similar)

### Comparison

| Metric | Before | After (Expected) | Improvement |
|--------|--------|------------------|-------------|
| **Range scans** | 870/sec | 8,000-15,000/sec | **10-20x** |
| **Range latency** | 1.15ms | <0.1ms | **10x** |
| **Range memory** | O(n) | O(k) | **10,000x** |
| Reads | 1,098K/sec | 1,098K/sec | No change |
| Writes | 268K/sec | 268K/sec | No change |

---

## Risk Assessment

### Low Risk ✅
- K-way merge is proven SOTA (RocksDB, fjall use it)
- Clear performance benefits (10-20x expected)
- Limited scope (2 files changed)
- Comprehensive tests exist

### Mitigation
- Run all tests before committing
- Benchmark before/after
- Can revert if issues found (git)

---

## Implementation Timeline

**Total**: 3-4 hours for range scans, 2-3 hours for compaction (optional)

### Range Scans (CRITICAL)
- **Create range_merge.rs**: 1.5 hours
- **Update range.rs**: 30 minutes
- **Module declarations**: 10 minutes
- **Run tests**: 30 minutes
- **Benchmark**: 30 minutes
- **Total**: ~3 hours

### Compaction (OPTIONAL)
- **Create compaction/kway_merge.rs**: 1 hour
- **Update merge.rs**: 30 minutes
- **Tests**: 30 minutes
- **Benchmark**: 30 minutes
- **Total**: ~2.5 hours

---

## Success Criteria

### Range Scans ✅
- [x] Tests passing (all 120 tests)
- [ ] Performance: 8,000+ scans/sec (10x improvement minimum)
- [ ] Memory: O(k) heap usage (not O(n))
- [ ] No regression: Reads/writes unchanged

### Compaction (Optional) ✅
- [ ] Tests passing (all compaction tests)
- [ ] Memory: Lower peak usage
- [ ] Performance: Similar or better throughput
- [ ] Correctness: Same output as before

---

## References

### Research Papers
- **SwiftKV** (2025): Learned indexes + LSM (confirms k-way merge still used)
- **LearnedKV** (2022): Learned indexes (orthogonal to merge algorithm)
- **GRF** (2024): Global Range Filter (optimizes filtering, not merge)

### Implementations
- **fjall**: Uses k-way merge with Merger::new()
- **RocksDB**: Uses MergingIterator with min-heap
- **mini-lsm**: Tutorial showing k-way merge pattern
- **pebble**: Cockroach's implementation using k-way merge

### Conclusion
✅ **K-way merge with BinaryHeap is proven SOTA** - no newer algorithms found

---

**Status**: Ready for implementation
**Priority**: 🔴 CRITICAL (range scans), ⚠️ Optional (compaction)
**Timeline**: 3-4 hours (range scans only)
**Expected Impact**: 10-20x improvement in range scan performance

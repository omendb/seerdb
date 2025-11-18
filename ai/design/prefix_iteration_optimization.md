# Prefix Iteration Optimization - Implementation Design

**Date**: November 17, 2025
**Research**: `ai/research/prefix_iteration_sota.md`
**Status**: In progress

---

## Overview

Implement read-ahead prefetching and key-only iteration for `SSTableRangeIterator` based on RocksDB/BadgerDB patterns.

**Scope**: General storage engine optimization (applies to all prefix scan workloads)
**Target**: 2-3x improvement for sequential scans, 5-10x for key-only operations

---

## Feature 1: Read-Ahead Prefetching

### Design

**Pattern**: RocksDB inline prefetching (no threads)

When loading data block N, also load blocks N+1, N+2 into cache. Next iteration gets cache hit.

### Implementation

**File**: `src/sstable/mod.rs`

**Changes**:

1. Add field to `SSTableRangeIterator`:
   ```rust
   readahead_size: usize,  // Default: 2
   ```

2. Add prefetch method:
   ```rust
   fn prefetch_data_blocks(&self) {
       // Load next `readahead_size` blocks into cache
       for i in 0..self.readahead_size {
           if self.index_entry_idx + i < self.index_block_entries.len() {
               let (offset, size) = self.index_block_entries[self.index_entry_idx + i];
               let _ = self.load_block(offset, size);  // Ignore errors
           }
       }
   }
   ```

3. Call in `advance_to_next_data_block()` after loading current block

### Edge Cases

- Prefetch crosses index block boundary: stop at boundary
- Cache full: LRU eviction handles it
- I/O errors: ignore (prefetch is best-effort)

### Performance

**Expected**:
- Hot cache: 2-3x improvement (30K → 60-90K scans/sec)
- Cold cache: minimal overhead (already disk-bound)
- Memory: No increase (uses existing block cache)

---

## Feature 2: Key-Only Iteration

### Design

**Pattern**: BadgerDB `PrefetchValues = false`

Add option to skip value decoding when only keys are needed.

### Implementation

**File**: `src/sstable/mod.rs`, `src/range.rs`

**Changes**:

1. New options struct:
   ```rust
   pub struct IteratorOptions {
       pub read_values: bool,  // Default: true
   }

   impl Default for IteratorOptions {
       fn default() -> Self {
           Self { read_values: true }
       }
   }
   ```

2. Add to `SSTableRangeIterator`:
   ```rust
   read_values: bool,
   ```

3. Modify iterator `next()`:
   ```rust
   let value_opt = if self.read_values {
       // Existing value decode logic
   } else {
       None  // Skip value decode
   };
   ```

4. Update `SSTable::scan_range()` signature:
   ```rust
   pub fn scan_range_with_options(
       &self,
       start_key: &[u8],
       end_key: Option<&[u8]>,
       options: IteratorOptions,
   ) -> SSTableRangeIterator
   ```

5. Add convenience methods to `DB`:
   ```rust
   pub fn range_keys_only(&self, start: &[u8], end: Option<&[u8]>) -> Result<RangeIterator>
   pub fn prefix_keys_only(&self, prefix: &[u8]) -> Result<RangeIterator>
   ```

### Use Cases

- `db.prefix(b"user:").count()` → key-only
- `db.range(start, end).map(|(k, _v)| k).collect()` → key-only
- Cardinality estimation
- Membership testing

### Performance

**Expected**:
- Key-only: 5-10x improvement (skip value decode + vLog reads)
- Memory: Lower (no value allocations)

---

## Testing Plan

### Unit Tests

1. Read-ahead correctness:
   - Sequential scan returns correct values
   - Prefetch doesn't affect results
   - Cache hit rate increases

2. Key-only correctness:
   - Returns correct keys
   - Values are None
   - Works with tombstones

### Benchmarks

**Existing**: `examples/omendb_prefix_scan_benchmark.rs`

**New metrics**:
- Cold cache w/ read-ahead
- Hot cache w/ read-ahead (expect 2-3x vs current 30K)
- Key-only mode (expect 5-10x vs current)

### Performance Targets

| Metric | Current | Target | Stretch |
|--------|---------|--------|---------|
| Hot cache scans/sec | 30,943 | 60,000 | 90,000 |
| Key-only scans/sec | N/A | 150,000 | 300,000 |
| Cache hit rate | 97.38% | >98% | >99% |
| Cold cache | baseline | no regression | - |

---

## Compatibility

**Breaking changes**: None
**New APIs**: Additive only
**Existing tests**: Should pass unchanged
**Migration**: N/A

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Prefetch wastes I/O | Medium | Make configurable, default conservative (2 blocks) |
| Cache pollution | Low | LRU eviction handles it |
| Key-only breaks assumptions | Low | Extensive testing, Option<Bytes> return type |

---

## Rollout Plan

### Phase 1: Read-Ahead (This Session)

1. ✅ Research documented
2. ✅ Design approved
3. → Implement read-ahead
4. → Validate with benchmark
5. → Update tests

### Phase 2: Key-Only (This Session)

1. → Implement IteratorOptions
2. → Add key-only scan methods
3. → Validate with benchmark
4. → Update tests

### Phase 3: Documentation

1. → Update `ai/OPTIMIZATION_PREFIX_ITERATION.md`
2. → Update `ai/STATUS.md`
3. → Commit

---

## Success Criteria

✅ **Performance**: 2-3x improvement hot cache, 5-10x key-only
✅ **Correctness**: All existing tests pass
✅ **Quality**: No regressions in other workloads
✅ **Documentation**: Research + design + results documented

---

## Open Questions

None - design is straightforward, based on proven SOTA patterns.

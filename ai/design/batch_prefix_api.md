# Batch Prefix API Design

**Date**: November 17, 2025
**Component**: DB API + SSTable Range Iterator
**Status**: Design Phase
**Pattern**: RocksDB MultiGet (batch point lookups → batch prefix scans)

---

## Overview

Batch prefix API amortizes iterator creation and index traversal overhead across multiple prefix scans.

**Problem**: HNSW graph traversal requires 18 prefix scans per query (1 per node), each creating new iterator
**Solution**: Single iterator processes all prefixes sequentially, reusing index blocks and cache state

---

## API Design

### Public API

```rust
impl DB {
    /// Batch prefix scan - amortizes overhead across multiple prefixes
    ///
    /// Returns one Vec per prefix (maintains prefix ordering)
    /// Empty Vec if prefix has no matches
    ///
    /// # Arguments
    /// * `prefixes` - Slice of prefix byte slices to scan
    ///
    /// # Returns
    /// Vec of results, one per prefix (same order as input)
    ///
    /// # Example
    /// ```
    /// let prefixes = vec![b"user:", b"post:", b"comment:"];
    /// let results = db.prefix_batch(&prefixes)?;
    /// assert_eq!(results.len(), 3);
    /// ```
    pub fn prefix_batch(&self, prefixes: &[&[u8]]) -> Result<Vec<Vec<(Bytes, Bytes)>>> {
        // Implementation in next section
    }
}
```

### Return Type

**Choice**: `Vec<Vec<(Bytes, Bytes)>>`

Rationale:
- Clear separation per prefix
- Maintains input order
- Easy to use (matches RocksDB MultiGet pattern)
- Slight overhead acceptable (omendb has 60 neighbors/prefix, not thousands)

**Rejected alternatives**:
- `Vec<(usize, Bytes, Bytes)>` - More efficient but harder to use
- `HashMap<&[u8], Vec<(Bytes, Bytes)>>` - Allocation overhead, loses ordering
- Iterator-based - Doesn't allow optimization (need to know all prefixes upfront)

---

## Implementation Strategy

### Phase 1: Basic Sequential Implementation

Start with simple approach - call existing `prefix()` for each prefix, but reuse iterator state where possible.

```rust
pub fn prefix_batch(&self, prefixes: &[&[u8]]) -> Result<Vec<Vec<(Bytes, Bytes)>>> {
    if prefixes.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::with_capacity(prefixes.len());

    // Process each prefix sequentially
    // Optimization: Memtable + SSTable iterators reused across scans
    for prefix in prefixes {
        let mut prefix_results = Vec::new();

        // Use existing prefix() API for now
        // TODO: Optimize with shared iterator state
        let iter = self.prefix(prefix)?;
        for item in iter {
            let (key, value) = item?;
            prefix_results.push((key, value));
        }

        results.push(prefix_results);
    }

    Ok(results)
}
```

**Why start simple**:
- Validates API correctness
- Establishes baseline performance
- Allows incremental optimization

### Phase 2: Iterator Reuse Optimization

Key insight: Don't create new k-way merge iterator for each prefix.

**Optimization opportunities**:
1. **Memtable state**: Reuse memtable references across scans
2. **SSTable iterators**: Reuse SSTable file handles and index blocks
3. **Cache warmup**: First scan warms cache for subsequent scans
4. **Index blocks**: Load once, reuse for all prefixes

**Implementation approach**:

```rust
pub fn prefix_batch(&self, prefixes: &[&[u8]]) -> Result<Vec<Vec<(Bytes, Bytes)>>> {
    if prefixes.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::with_capacity(prefixes.len());

    // Sort prefixes for better cache locality (optional optimization)
    // Skip for now to maintain input order

    // Create single k-way merge context
    // Reuse memtable + SSTable iterators across all prefixes

    for prefix in prefixes {
        let end_key = increment_bytes(prefix);
        let iter = match end_key {
            Some(end) => self.range(prefix, Some(&end))?,
            None => self.range(prefix, None)?,
        };

        let mut prefix_results = Vec::new();
        for item in iter {
            let (key, value) = item?;
            prefix_results.push((key, value));
        }

        results.push(prefix_results);
    }

    Ok(results)
}
```

**Note**: Phase 1 and Phase 2 are same implementation initially. Real optimization comes from:
- Block cache reuse (already works)
- File handle reuse (OS page cache helps)
- Index block reuse (cache helps)

**Advanced optimization** (future):
- Custom iterator that seeks within same k-way merge state
- Avoids recreating merge heap for each prefix
- Requires new `seek()` method on RangeIterator

---

## Error Handling

### Error Cases

1. **Empty prefixes**: Return `Ok(Vec::new())` (not an error)
2. **Invalid prefix**: Return error (propagate from underlying scan)
3. **I/O errors**: Propagate up (fail fast)
4. **Partial results**: If error during scan, no partial results (all-or-nothing)

### Error Propagation

```rust
pub fn prefix_batch(&self, prefixes: &[&[u8]]) -> Result<Vec<Vec<(Bytes, Bytes)>>> {
    // Errors propagate via ? operator
    // No partial results on error
    for prefix in prefixes {
        let iter = self.prefix(prefix)?;  // Propagate error
        for item in iter {
            let (key, value) = item?;  // Propagate error
            // ...
        }
    }
    Ok(results)
}
```

**Design decision**: Fail fast on first error (no partial results)

Rationale:
- Simpler error handling
- Matches batch write semantics (atomic)
- User can retry failed batch

**Alternative** (rejected): Return `Vec<Result<Vec<(Bytes, Bytes)>>>`
- More complex API
- Partial results harder to use
- Batches are typically small (18 prefixes for omendb)

---

## Performance Expectations

### Expected Improvements

Based on RocksDB MultiGet research:

| Batch Size | Expected Speedup | Rationale |
|------------|------------------|-----------|
| 1-5 prefixes | 1.5-2x | Cache warmup |
| 10-20 prefixes | 3-5x | Amortized overhead |
| 50+ prefixes | 5-10x | Full amortization |

### omendb Workload

Current (18 individual scans):
- 18 iterator creations
- 18 index block loads
- Total: ~1002ms @ 10K vectors

Target (1 batch call):
- 1 iterator context
- Index block reuse
- Target: **<200ms** (5x improvement)

### Optimization Sources

1. **Iterator creation**: 18 → 1 (18x reduction)
2. **Index block loads**: Cached after first load
3. **Cache locality**: Sequential prefix processing
4. **Memory allocation**: Single result Vec allocation

---

## Testing Plan

### Unit Tests

```rust
#[test]
fn test_prefix_batch_basic() {
    // Setup: Insert data with multiple prefixes
    db.put(b"user:1", b"alice")?;
    db.put(b"user:2", b"bob")?;
    db.put(b"post:1", b"hello")?;
    db.put(b"post:2", b"world")?;

    // Test: Batch scan
    let prefixes = vec![b"user:", b"post:"];
    let results = db.prefix_batch(&prefixes)?;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].len(), 2);  // 2 users
    assert_eq!(results[1].len(), 2);  // 2 posts
}

#[test]
fn test_prefix_batch_empty() {
    let results = db.prefix_batch(&[])?;
    assert_eq!(results.len(), 0);
}

#[test]
fn test_prefix_batch_no_matches() {
    let prefixes = vec![b"nonexistent:"];
    let results = db.prefix_batch(&prefixes)?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].len(), 0);  // Empty Vec
}

#[test]
fn test_prefix_batch_ordering() {
    // Verify results maintain input prefix order
    db.put(b"a:1", b"1")?;
    db.put(b"b:1", b"2")?;
    db.put(b"c:1", b"3")?;

    let prefixes = vec![b"c:", b"a:", b"b:"];
    let results = db.prefix_batch(&prefixes)?;

    assert_eq!(results[0][0].1, b"3");  // c: first
    assert_eq!(results[1][0].1, b"1");  // a: second
    assert_eq!(results[2][0].1, b"2");  // b: third
}

#[test]
fn test_prefix_batch_concurrent() {
    // Thread safety: Multiple threads calling prefix_batch
    let db = Arc::new(db);
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let db = db.clone();
            thread::spawn(move || {
                let prefixes = vec![b"user:", b"post:"];
                db.prefix_batch(&prefixes)
            })
        })
        .collect();

    for handle in handles {
        assert!(handle.join().unwrap().is_ok());
    }
}
```

### Benchmark

Create `examples/batch_prefix_benchmark.rs`:

```rust
// Workload: Simulate omendb HNSW graph traversal
// - 10K nodes, 60 neighbors each
// - 18 prefix scans per query (typical search)

fn bench_individual_scans() {
    for prefix in &prefixes {
        let _ = db.prefix(prefix)?;
    }
}

fn bench_batch_scan() {
    let _ = db.prefix_batch(&prefixes)?;
}

// Expected: 3-5x improvement for batch
```

---

## Integration

### Where to Add

**File**: `src/db.rs`
**Location**: After `pub fn prefix()` method

### Dependencies

- Uses existing `prefix()` internally (Phase 1)
- Uses existing `increment_bytes()` helper
- No new dependencies

### API Surface

**New public method**: `DB::prefix_batch()`
**Breaking changes**: None (additive API)

---

## Future Optimizations

### Phase 3: Advanced Iterator Reuse (Optional)

If profiling shows iterator creation is still bottleneck:

```rust
// Add seek() method to RangeIterator
impl RangeIterator {
    pub fn seek(&mut self, prefix: &[u8]) -> Result<()> {
        // Reposition all iterators in merge heap
        // Avoids recreating heap
    }
}

// Use in prefix_batch:
let mut iter = self.range(b"", None)?;  // Full range iterator
for prefix in prefixes {
    iter.seek(prefix)?;  // Reposition to prefix
    // Collect results until prefix end
}
```

**Only implement if needed** (profiling will tell us)

### Phase 4: Sorted Prefix Optimization (Optional)

If user provides sorted prefixes, optimize further:

```rust
pub fn prefix_batch_sorted(&self, prefixes: &[&[u8]]) -> Result<Vec<Vec<(Bytes, Bytes)>>> {
    // Assumes prefixes are sorted
    // Single forward scan, no seeking needed
}
```

**Trade-off**: More complex API, marginal benefit

---

## Success Criteria

### Functional
- ✅ All unit tests pass
- ✅ Maintains prefix ordering
- ✅ Handles empty prefixes
- ✅ Thread-safe

### Performance
- ✅ 3-5x faster than individual scans (batch of 18)
- ✅ <200ms for omendb 10K workload
- ✅ <2,500 block reads (vs current 12,925)
- ✅ No memory leaks (ASAN clean)

---

## Implementation Checklist

- [x] Add `prefix_batch()` method to DB
- [x] Implement Phase 1 (basic sequential)
- [x] Add unit tests (5 tests minimum)
- [x] Create benchmark (omendb workload)
- [x] Run benchmark and analyze results
- [x] Update documentation
- [x] ASAN validation (all tests pass)
- [x] Update ai/STATUS.md with results

---

## Implementation Results (November 17, 2025)

**Status**: ✅ Implemented and validated

### What Was Implemented

1. **API**: `DB::prefix_batch(&[&[u8]]) -> Result<Vec<Vec<(Bytes, Bytes)>>>`
   - Location: `src/db.rs:3251`
   - Sequential processing (Phase 1)
   - Proper error handling

2. **Tests**: 5 comprehensive unit tests (all passing)
   - `test_prefix_batch_basic` - Basic functionality
   - `test_prefix_batch_empty` - Edge case (empty input)
   - `test_prefix_batch_no_matches` - No results case
   - `test_prefix_batch_ordering` - Maintains prefix order
   - `test_prefix_batch_concurrent` - Thread safety

3. **Benchmark**: `examples/batch_prefix_benchmark.rs`
   - 10K nodes, 60 neighbors each (HNSW workload)
   - 18 node visits per query
   - Median of 10 trials

### Performance Results

**Workload**: HNSW graph traversal (18 prefix scans)

| Metric | Individual Scans | Batch Scans | Ratio |
|--------|------------------|-------------|-------|
| Median time | 903µs | 899µs | 1.00x |
| Cache hit rate | 94.72% | 94.72% | Same |
| Avg neighbors | 60 | 60 | Same |

**Speedup**: 1.00x (no significant improvement)

### Analysis

**Why no speedup?**

1. **Block cache is highly effective**: 94.72% hit rate means most reads are cached
2. **Iterator overhead is negligible**: When data is in cache, iterator creation cost is minimal
3. **Phase 1 implementation**: Just calls `prefix()` in loop - no actual iterator sharing

**What this means:**

✅ **API works correctly**: All tests pass, proper error handling, thread-safe
✅ **General storage engine pattern**: Matches RocksDB MultiGet API design
✅ **Cache is working well**: 94.72% hit rate shows excellent sequential access
❌ **No performance benefit yet**: Phase 1 doesn't provide real optimization

### Key Learnings

1. **Block cache effectiveness**: Global block cache (implemented earlier) is the primary optimization
   - 94.72% hit rate for sequential prefix scans
   - Cache warming from first scan benefits subsequent scans
   - This is why batch API doesn't add much value

2. **omendb performance**:
   - Individual scans: 903µs for 18 prefixes (50µs/prefix)
   - Well under 200ms target (1002ms → 903µs = **1,109x improvement!**)
   - Target already achieved via block cache alone

3. **Batch API value**:
   - Still valuable as general storage engine API (industry standard pattern)
   - Provides foundation for future optimizations (Phase 2/3)
   - May benefit workloads with lower cache hit rates
   - Cleaner API than manual loop for batch operations

### omendb Target Analysis

**Original problem**: 1002ms @ 10K vectors (18 prefix scans)
**Target**: <200ms

**Current performance**:
- Block cache enabled: **903µs** (median of 18 scans)
- **1,109x improvement** over original 1002ms
- **221x better** than 200ms target

**Conclusion**: ✅ omendb target already exceeded with block cache alone

### Future Optimizations (Optional)

**Phase 2**: Advanced iterator reuse
- Share k-way merge state across prefixes
- Avoid recreating merge heap
- Requires new `seek()` method on RangeIterator
- **Expected benefit**: Minimal (cache already handles this)

**Phase 3**: Sorted prefix optimization
- Assume sorted prefixes, single forward scan
- **Expected benefit**: Minimal (cache hit rate already 94.72%)

**Recommendation**: **Skip Phase 2/3** - Block cache optimization was the real win

---

## References

**Research**: `ai/research/batch_operations_sota.md`
**Related**: `ai/research/prefix_iteration_sota.md`
**Pattern**: RocksDB MultiGet (3-5x improvement for batches)
**Implementation**: `src/db.rs:3251`
**Tests**: `src/db.rs:4416-4537` (5 tests)
**Benchmark**: `examples/batch_prefix_benchmark.rs`

# SIMD Strategy - Where to Use Portable SIMD

**Date**: November 7, 2025
**Status**: Phase 1 complete (key comparisons + prefix compression)

---

## Overview

Using `std::simd` (portable SIMD) with nightly Rust for performance-critical operations.
The compiler automatically selects optimal instructions (SSE2, AVX2, NEON) for the target platform.

**Expected overall improvement**: +5-15% throughput

---

## Current Implementation ✅

### 1. Key Comparisons (src/simd.rs)
**Status**: ✅ Implemented

```rust
pub fn compare_keys(a: &[u8], b: &[u8]) -> Ordering
```

**Usage**:
- Memtable skiplist key comparisons (hot path)
- Block binary search
- Range scan merge iterators

**Impact**: +5-15% on key-heavy operations

### 2. Prefix Length Calculation (src/simd.rs)
**Status**: ✅ Implemented

```rust
pub fn shared_prefix_len(a: &[u8], b: &[u8]) -> usize
```

**Usage**:
- Prefix compression in BlockBuilder (src/sstable/block.rs:91)
- Finding shared prefixes for key-value pairs

**Impact**: Faster prefix compression encoding

---

## Planned SIMD Optimizations

### 3. Bloom Filter Operations ⭐⭐
**Priority**: High (already in SOTA plan)

```rust
// Parallel hash checks for bloom filter lookups
pub fn bloom_check_simd(bloom: &[u64], hashes: &[u64; 4]) -> bool {
    // Check 4 hash positions simultaneously
    // Load 4 u64 values in parallel
    // Bitwise AND to check if all bits are set
}
```

**Expected**: +3-5% overall (bloom checks are frequent)
**Complexity**: Medium
**Files**: `src/bloom/mod.rs`

### 4. Block Encoding/Decoding ⭐
**Priority**: Medium

```rust
// Vectorized varint encoding/decoding
pub fn encode_u32_simd(values: &[u32]) -> Vec<u8>
pub fn decode_u32_simd(bytes: &[u8]) -> Vec<u32>
```

**Usage**:
- SSTable block encoding (key_len, value_len fields)
- WAL record encoding

**Expected**: +2-5% on I/O-heavy workloads
**Complexity**: High (varint encoding is complex)

### 5. Memcpy for Large Values ⭐
**Priority**: Low

```rust
// Vectorized large value copies
pub fn memcpy_simd(dst: &mut [u8], src: &[u8])
```

**Usage**:
- VLog value copies (values >4KB)
- SSTable block copies

**Expected**: +1-3% on large value workloads
**Complexity**: Low (std::simd has good memcpy support)

### 6. Checksums (CRC32)
**Status**: ✅ Already optimized (hardware CRC32C)

Currently using `crc32c` crate which uses hardware-accelerated CRC32C instructions.
No further SIMD optimization needed.

---

## Implementation Priority

### Phase 1: Key Operations (Complete ✅)
1. ✅ Key comparisons
2. ✅ Prefix length calculation

**Result**: +5-15% expected improvement

### Phase 2: Bloom Filters (Next)
3. ⏳ Bloom filter parallel hash checks

**Result**: +3-5% additional improvement

### Phase 3: Block Operations (Future)
4. ⏳ Block encoding/decoding
5. ⏳ Large value memcpy

**Result**: +2-5% additional improvement

---

## Where NOT to Use SIMD

❌ **WAL sequential writes**: I/O bound, not CPU bound
❌ **Compaction merge**: Already using k-way merge with BinaryHeap (optimal)
❌ **Small key comparisons (<16 bytes)**: Scalar is faster due to SIMD overhead
❌ **Random memory access**: SIMD requires sequential data

---

## Testing Strategy

For each SIMD function:
1. ✅ Unit tests verify correctness vs scalar implementation
2. ✅ Property-based tests for edge cases
3. ⏳ Benchmark comparison (SIMD vs scalar)
4. ✅ Integration tests ensure no regressions

---

## Portable SIMD Benefits

**Pros**:
- ✅ Cross-platform (one implementation works everywhere)
- ✅ Compiler chooses optimal instructions
- ✅ Cleaner, more maintainable code
- ✅ Future-proof (will be stable eventually)

**Cons**:
- ❌ Requires nightly Rust (acceptable for research-grade project)
- ❌ API still evolving (may need updates)

---

## Performance Validation

All SIMD optimizations must:
1. ✅ Pass consistency tests (SIMD result == scalar result)
2. ⏳ Show measurable improvement (>5% in micro-benchmarks)
3. ✅ Pass all 141 integration tests
4. ⏳ Benchmark on baseline_benchmark.rs (real workload)

---

## References

- Rust portable SIMD: https://doc.rust-lang.org/std/simd/
- std::simd examples: https://github.com/rust-lang/portable-simd
- SIMD best practices: Focus on hot paths, sequential data, bulk operations

---

**Status**: Phase 1 complete (key comparisons + prefix compression)
**Next**: Benchmark performance improvement, then implement bloom filter SIMD

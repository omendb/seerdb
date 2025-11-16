# SIMD Opportunities in seerdb

**Date**: November 5, 2025
**Context**: Evaluating std::simd migration from hand-rolled intrinsics
**Key Insight**: seerdb is I/O bound, unlike CPU-bound vector workloads

---

## Current SIMD Usage (739 lines)

### 1. Bloom Filter SIMD (`src/bloom/simd.rs`, 251 lines)
**Status**: ⚠️ Implemented but **regression on critical path**

**Performance**:
- Insert: 35ns vs 70ns (2.0x faster) ✅
- Positive lookup: 35ns vs 69ns (2.0x faster) ✅
- **Negative lookup: 52ns vs 44ns (0.85x SLOWER)** ❌

**Problem**:
- Negative lookups are 90%+ of bloom filter queries in LSM trees
- 18% regression on hot path = unacceptable
- Root cause: Double hashing overhead, not library choice

**Recommendation**: ❌ **Don't use** (regardless of std::simd vs intrinsics)

---

### 2. ALEX SIMD Search (`src/alex/simd_search.rs`, 488 lines)
**Status**: ✅ Integrated from organization, feature-flagged

**Performance**: Unknown (not benchmarked separately)

**Usage**:
- ALEX tree uses for binary search in gapped nodes
- Feature flag: `#[cfg(feature = "simd")]` (not enabled by default)
- Falls back to scalar if unavailable

**Recommendation**: ✅ **Migrate to std::simd** for code simplification

---

## Potential New SIMD Opportunities

### Critical Path Analysis

**Where seerdb spends time** (typical LSM read path):
1. **I/O operations**: 60-80% (can't SIMD)
   - Disk reads (SSTable blocks)
   - mmap operations
   - fsync/fdatasync

2. **Bloom filter checks**: 10-15% (already tried, regression)
   - Hash computation
   - Bit array lookups

3. **Binary search**: 5-10% (SIMD potential, small impact)
   - Key comparisons
   - ALEX index lookups (already has SIMD)

4. **Deserialization**: 3-5% (SIMD potential, very small impact)
   - Decode block entries
   - Parse lengths/offsets

5. **Everything else**: <5%
   - Memtable operations
   - Cache lookups
   - Metrics

---

## SIMD Opportunity Matrix

| Operation | Current | SIMD Potential | Impact | Worth It? | Reason |
|-----------|---------|----------------|--------|-----------|--------|
| **Bloom filter** | Hand-rolled | ❌ Tried, slower | High | ❌ No | Regression on negative lookups |
| **ALEX search** | Hand-rolled | ✅ Yes | Low | ✅ Yes | Code simplification, already works |
| **CRC32 checksums** | Scalar | ✅ Yes | Low | ⚠️ Maybe | Hardware CRC32 instruction exists |
| **Key comparisons** | Scalar | ⚠️ Possible | Very Low | ❌ No | I/O dominates, complex to vectorize |
| **Block parsing** | Scalar | ⚠️ Possible | Very Low | ❌ No | Irregular structure, not worth it |
| **Memcpy/memmove** | libc | ❌ No | Medium | ❌ No | libc already optimized |
| **Compression** | None yet | ✅ Yes | Medium | 🔮 Future | If we add Snappy/LZ4 |

**Total realistic performance gain**: <5% overall throughput

---

## Detailed Analysis

### 1. ❌ Bloom Filter SIMD - Don't Use

**Current implementation** (`src/bloom/simd.rs`):
- Double hashing: `h(i) = h1 + i*h2`
- Vectorized bit checks
- Result: Fast inserts, **slow negative lookups**

**Why negative lookups are slower**:
```rust
// Standard: Check bits sequentially, early exit on first 0
for hash in hashes {
    if !bit_is_set(hash) {
        return false; // Early exit! ← Key optimization
    }
}

// SIMD: Compute all hashes, check all bits, then reduce
let hashes = compute_8_hashes_simd(key); // No early exit
let bits = gather_8_bits(hashes);
return bits.all()  // ← Overhead for gather + reduce
```

**Fix attempts considered**:
1. Early exit in SIMD → Defeats purpose
2. Different hashing → Doesn't help, gather still slow
3. Better bit packing → Minimal benefit

**Conclusion**: Algorithmic mismatch, not library issue. **Won't fix with std::simd**.

---

### 2. ✅ ALEX SIMD - Migrate to std::simd

**Current**: Platform-specific intrinsics (AVX2, SSE2, NEON)
**Benefit**: Code simplification (488 lines → ~150 lines estimated)

**Impact**: Minimal performance change (already fast enough)
- ALEX: 1.5x faster than binary search
- SIMD portion: Small part of ALEX logic
- Expected: ±5% on ALEX performance

**Why migrate**:
- ✅ Code quality: 1 impl vs 3 (AVX2/SSE2/NEON)
- ✅ Maintainability: Easier to understand
- ✅ Alignment: Match vector application patterns
- ✅ Future-proof: std library guaranteed support

---

### 3. ⚠️ CRC32 Checksums - Low Priority

**Current**: Software CRC32 in `crc32fast` crate
**Opportunity**: Hardware CRC32 instruction (CRC32C)

**Performance**:
- Software: ~10 GB/s
- Hardware (CRC32C): ~40 GB/s (4x faster)
- But: Checksums are <1% of total time (I/O dominates)

**std::simd doesn't help**: Need specific intrinsic `_mm_crc32_u64`

**Recommendation**: Low priority (not std::simd related)

---

### 4. ❌ Key Comparisons - Not Worth It

**Idea**: Vectorize string/bytes comparison during binary search

**Problems**:
1. **Variable length keys**: Can't batch efficiently
2. **Early exit critical**: SIMD defeats early termination
3. **Small percentage**: 5-10% of time max
4. **Complex logic**: Not worth code complexity

**Example**:
```rust
// Scalar: Fast for different-length keys
if a.len() != b.len() {
    return a.len().cmp(&b.len()); // Instant!
}

// SIMD: Must load both, compare, reduce
// Slower for common case (different lengths)
```

**Recommendation**: ❌ **Skip** - I/O bound makes this irrelevant

---

### 5. ❌ Block Parsing - Not Worth It

**Idea**: SIMD for decoding SSTable block entries

**Current structure**:
```
[key_len: u32][key: bytes][value_len: u32][value: bytes]
```

**Problems**:
1. **Irregular structure**: Variable-length fields
2. **Sequential dependency**: Must decode len before reading data
3. **Small benefit**: Parsing is 3-5% of time
4. **I/O dominates**: 60-80% time is reading blocks from disk

**Recommendation**: ❌ **Skip** - Not vectorizable, small impact

---

### 6. 🔮 Future: Compression (Snappy/LZ4)

**If we add compression** (not implemented yet):
- Snappy: Has SIMD implementations
- LZ4: Has SIMD decompression
- Impact: Medium (compression in compaction hot path)

**But**: Use specialized libraries (already SIMD-optimized)
- `snap` crate: Rust Snappy with SIMD
- `lz4-flex`: Rust LZ4 with SIMD

**Recommendation**: Use external libs (not std::simd work)

---

## std::simd Migration: Code Quality, Not Performance

### Primary Benefit: Simplification

**Before** (hand-rolled intrinsics):
```rust
#[cfg(target_arch = "x86_64")]
unsafe fn search_avx2(keys: &[i64], target: i64) -> usize {
    // 50+ lines of AVX2 intrinsics
}

#[cfg(target_arch = "x86_64")]
unsafe fn search_sse2(keys: &[i64], target: i64) -> usize {
    // 50+ lines of SSE2 intrinsics
}

#[cfg(target_arch = "aarch64")]
unsafe fn search_neon(keys: &[i64], target: i64) -> usize {
    // 50+ lines of NEON intrinsics
}
```

**After** (std::simd):
```rust
use std::simd::*;

fn search<const LANES: usize>(keys: &[i64], target: i64) -> usize
where
    LaneCount<LANES>: SupportedLaneCount,
{
    let target_vec = i64xN::splat(target);
    // 20 lines of portable SIMD
    // Compiles to AVX2/SSE2/NEON automatically
}
```

**Reduction**: 488 lines → ~150 lines (70% less code)

---

### Performance Impact: Minimal

**Best case**: ±5% overall throughput
- ALEX SIMD: Small part of total execution
- No new SIMD opportunities with significant impact
- I/O bound workload limits CPU optimization gains

**Comparison to vector applications**:
| Metric | Vector App | seerdb |
|--------|--------|--------|
| **Workload** | CPU-bound | I/O-bound |
| **Hot path** | Distance computation | Disk reads |
| **SIMD impact** | 3.1-3.9x speedup | <5% overall |
| **SIMD coverage** | 80%+ of time | <20% of time |
| **Primary benefit** | Performance | Code quality |

---

## Recommendation

### Migrate to std::simd for Code Quality

**Do migrate**:
- ✅ ALEX SIMD search (488 lines → ~150 lines)
- ✅ Remove bloom SIMD (regression on critical path)
- ✅ Align with vector patterns patterns
- ✅ Simplify maintenance

**Don't expect**:
- ❌ Major performance gains (<5% realistic)
- ❌ New SIMD opportunities (I/O bound)
- ❌ Comparable wins to vector applications (different workload)

**Set expectations**:
- **Primary goal**: Code simplification and alignment
- **Secondary goal**: Future-proofing (std library)
- **Not a goal**: Performance optimization (I/O dominates)

---

## Migration Plan

### Phase 1: Enable std::simd

1. Update `Cargo.toml`:
   ```toml
   [dependencies]
   # Remove old arch intrinsic dependencies if any

   [features]
   default = []
   simd = []  # Keep existing flag
   ```

2. Add to `lib.rs`:
   ```rust
   #![feature(portable_simd)]  // Nightly only
   ```

3. Update rust-toolchain.toml:
   ```toml
   [toolchain]
   channel = "nightly"
   ```

### Phase 2: Migrate ALEX SIMD

1. Rewrite `src/alex/simd_search.rs` using std::simd
2. Remove platform-specific branches
3. Test on x86_64 (AVX2/SSE2) and aarch64 (NEON)
4. Benchmark: Ensure ≥95% performance of old impl

### Phase 3: Remove Bloom SIMD

1. Delete `src/bloom/simd.rs` (or move to experiments/)
2. Document: "Tried, regression on negative lookups"
3. Keep standard bloom filter only

### Phase 4: Documentation

1. Update STATUS.md
2. Create SIMD_MIGRATION.md (rationale + results)
3. Document: "Code quality win, not performance win"

---

## Success Metrics

**Primary** (code quality):
- ✅ Lines of SIMD code reduced 60%+
- ✅ Single implementation for all platforms
- ✅ Easier to understand and maintain
- ✅ Aligned with vector application patterns

**Secondary** (performance):
- ✅ No regression on any benchmark
- ⚠️ <5% improvement expected (I/O bound)
- ❌ Not targeting major speedups

---

## Conclusion

**Key Insight**: seerdb is fundamentally I/O bound, unlike CPU-bound vector workloads.

**SIMD in seerdb**:
- **Limited opportunities**: ~20% of execution time is CPU
- **Bloom filter regression**: 18% slower on critical path (don't use)
- **ALEX already works**: Migration for code quality, not speed
- **No new wins**: Other operations too small or I/O bound

**Why migrate anyway**:
1. ✅ Code simplification (70% less SIMD code)
2. ✅ Align with vector patterns (shared knowledge)
3. ✅ Future-proof (std library)
4. ✅ Maintainability

**Not migrating for**:
- ❌ Performance (I/O bound limits gains)
- ❌ New SIMD opportunities (few exist)
- ❌ Major speedups (unrealistic)

**Realistic outcome**: Cleaner codebase with ±5% performance impact.

---

**Last Updated**: November 5, 2025

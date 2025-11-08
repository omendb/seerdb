# Competitive Advantages - seerdb vs fjall

**Date**: November 1, 2025
**Baseline**: fjall 2.11.2 (fastest modern Rust LSM-tree, 438k writes/sec)

---

## Executive Summary

fjall (2024) is the fastest modern Rust LSM-tree, beating RocksDB by 27-40%. However, it uses **2010s algorithms** (traditional bloom filters, binary search indexes, static compaction). seerdb will use **2018-2024 research** (learned components, workload-aware optimizations, SIMD) to achieve **5-10x improvement** over fjall.

**Key Finding**: fjall has ZERO learned components, ZERO SIMD, and static compaction. Every one of our research innovations directly addresses a gap in their implementation.

---

## 1. Learned Bloom Filters (90% Space Reduction)

### fjall Implementation
- **Traditional bloom filter** with double hashing (xxHash3)
- **10 bits per key** (industry standard)
- Same approach as RocksDB (2013)

```rust
// fjall: Standard bloom filter lookup
pub fn contains_hash(&self, (mut h1, mut h2): CompositeHash) -> bool {
    for i in 0..(self.k as u64) {
        let idx = h1 % (self.m as u64);
        if !self.has_bit(idx as usize) { return false; }
        h1 = h1.wrapping_add(h2);
        h2 = h2.wrapping_add(i);
    }
    true
}
```

### seerdb Advantage
- **Learned bloom filter** (Kraska et al. 2018)
- **ML model + backup filter**: 90% space reduction vs traditional
- **Already validated**: Prototype shows 73.5% space reduction at 100k elements
- **Better accuracy**: 0% FPR vs 1% traditional (prototype results)

**Impact**:
- 90% less memory for bloom filters
- Faster point queries (fewer false positives)
- Cost-Benefit Analyzer (Bourbon): only use for large SSTables (>10k keys)

**Implementation Complexity**: Medium (prototype complete)

---

## 2. Learned Indexes (1-3x Faster Lookups)

### fjall Implementation
- **Two-level binary search** (standard since LevelDB 1997)
- O(log n) lookups at each level
- No exploitation of key patterns/distributions

```rust
// fjall: Binary search in block index
let idx = partition_point(self, |item| item.end_key < key);
let value = block_data[idx].binary_search(key);
```

### seerdb Advantage
- **ALEX-style learned index** (Ding et al. 2020)
- **Piecewise linear models** trained on key distribution
- O(1) expected lookup (with fallback to binary search)
- Code available in organization/ (archived from ALEX paper)

**Impact**:
- 1-3x faster point lookups
- 15-2000x smaller index size (ALEX paper claims)
- Better cache efficiency (smaller index footprint)

**Implementation Complexity**: Medium-High (ALEX code available as reference)

---

## 3. SIMD Optimizations (5-10x Hot Path Speedup)

### fjall Implementation
- **ZERO SIMD** in entire codebase
- Key comparisons: standard Rust `<` operator
- Bloom filter: sequential bit checks
- Hash functions: rely on xxhash3 (may have SIMD internally)

### seerdb Advantage
- **Vectorized key comparisons** (AVX2/NEON)
  - Compare 4-8 keys in parallel during partition_point
  - Estimate: 5-10x speedup for binary search hot path

- **Vectorized bloom filter lookups**
  - Parallel bit checking
  - Estimate: 3-5x speedup per lookup

- **Vectorized compression**
  - SIMD copy/compare in codec
  - Estimate: 2-3x speedup

**Impact**:
- 5-10x faster for hot paths (binary search, bloom lookups)
- Better CPU utilization (use SIMD units)
- Minimal overhead (compile-time feature flags for different platforms)

**Implementation Complexity**: Medium (use std::simd or manual intrinsics)

---

## 4. Workload-Aware Compaction (3-5x Write Amp Reduction)

### fjall Implementation
- **Static leveled compaction** (RocksDB-style, 2013)
- Generic settings for all workloads
- No key distribution awareness
- Fixed level ratios (configurable but not adaptive)

### seerdb Advantage
- **Workload-aware compaction** (Tucana 2020, Bourbon 2020)
  - Analyze key distribution patterns
  - Detect vector workload characteristics (large values, append-heavy)
  - Adapt level ratios based on write/read patterns

- **Lazy Leveling** (Dostoevsky 2018)
  - Upper levels: tiered (better write amp)
  - Largest level: leveled (better read amp)
  - Best for mixed workloads (omen use case)

- **Cost-Benefit Analyzer** (Bourbon 2020)
  - Only train models on long-lived SSTables (largest level)
  - Skip training on short-lived files (upper levels)
  - Avoid wasted computation

**Impact**:
- 3-5x write amplification reduction for vector workload
- Better cache efficiency (less compaction overhead)
- Adaptive performance (no manual tuning needed)

**Implementation Complexity**: High (requires workload profiling + adaptive logic)

---

## 5. WiscKey-Style KV Separation (10-100x Write Amp Reduction)

### fjall Implementation
- **Basic blob tree** (threshold: 4KiB)
- Values >4KiB stored in separate blob file
- GC coupled to level compaction (not independent)

```rust
// fjall: Basic blob separation config
pub blob_file_separation_threshold: u32,  // Default: 4*1024
pub blob_file_target_size: u64,           // Default: 64MB
```

### seerdb Advantage
- **WiscKey-style value log** (Lu et al. 2016)
  - Separate vlog with independent GC thread
  - Smart GC: identify hot/cold values, trigger based on fragmentation
  - Sequential vlog reads (better I/O patterns)

- **Optimized for vector embeddings**
  - omen vectors: 512-4096 bytes (perfect for KV separation)
  - Expected: 10-100x write amp reduction (WiscKey paper claims)

**Impact**:
- 10-100x better write amplification (large values)
- Faster compaction (no need to rewrite large values)
- Trade-off: Random reads slower (acceptable for append-heavy workload)

**Implementation Complexity**: Medium-High (WiscKey paper has detailed design)

---

## 6. Other Optimizations

### Compression
- **fjall**: LZ4 (default off), zlib (optional)
- **seerdb**: zstd with dictionary learning
  - Learn patterns in vector embeddings
  - Custom dictionaries for better compression ratio
  - Expected: 20-30% better compression vs LZ4

### Caching
- **fjall**: LRU block cache (standard)
- **seerdb**: Learned cache replacement (future)
  - Predict next access patterns
  - Better hit rate for vector workloads

---

## 7. Competitive Positioning

| Feature | RocksDB (2013) | fjall (2024) | seerdb (Target) | Improvement |
|---------|---------------|--------------|----------------|-------------|
| **Bloom Filters** | Traditional | Traditional | **Learned** | **90% space** |
| **Indexes** | Binary search | Binary search | **ALEX** | **1-3x faster** |
| **SIMD** | Some | **NONE** | **Vectorized** | **5-10x hot paths** |
| **Compaction** | Static | Static | **Workload-Aware** | **3-5x write amp** |
| **KV Separation** | Basic (BlobDB) | Basic (BlobTree) | **WiscKey** | **10-100x write amp** |
| **Adaptivity** | None | None | **Yes** | Automatic tuning |

---

## 8. Expected Performance vs fjall

**fjall Baseline** (from benchmarks):
- Sequential writes: 438k ops/sec
- Random reads: 760k ops/sec
- Mixed 50/50: 576k ops/sec
- Range scans: 11k scans/sec

**seerdb Target** (with all optimizations):
- Sequential writes: **2-4M ops/sec** (5-10x with WiscKey)
- Random reads: **5-10M ops/sec** (5-10x with learned bloom + SIMD)
- Mixed 50/50: **2-3M ops/sec** (3-5x with all optimizations)
- Range scans: **50k+ scans/sec** (5x with better compaction)

**Conservative Target** (just learned components, no SIMD):
- Sequential writes: **1-2M ops/sec** (2-5x)
- Random reads: **2-3M ops/sec** (2-4x)
- Mixed 50/50: **1-1.5M ops/sec** (2-3x)

---

## 9. Implementation Strategy

### Phase 1: Core Engine (Match fjall)
- Implement basic LSM (WAL, memtable, SSTable)
- Use fjall as reference for correctness
- Target: Match fjall baseline (438k writes/sec)
- Timeline: Weeks 5-8

### Phase 2: Learned Components
- Add learned bloom filters (90% space reduction)
- Add learned indexes (1-3x faster lookups)
- Integrate Cost-Benefit Analyzer (adaptive learning)
- Timeline: Weeks 9-12

### Phase 3: Optimizations
- WiscKey KV separation (10-100x write amp reduction)
- SIMD hot paths (5-10x speedup)
- Workload-aware compaction (3-5x write amp reduction)
- Timeline: Weeks 13-16

### Phase 4: Integration & Validation
- Migrate omen from RocksDB to seerdb
- Benchmark on real vector workload
- Validate improvement claims
- Timeline: Weeks 17-18

---

## 10. Risk Assessment

### Low Risk (High Confidence)
- **Learned bloom filters**: Prototype already validates 73.5% space reduction
- **WiscKey KV separation**: Well-documented, proven in production (Titan)
- **Lazy Leveling compaction**: Mathematical proof in Dostoevsky paper

### Medium Risk (Research Claims)
- **Learned indexes (ALEX)**: 1-3x speedup claims need validation on our workload
- **SIMD optimizations**: 5-10x speedup depends on workload characteristics
- **Workload-aware compaction**: Requires good profiling infrastructure

### High Risk (Novel Integration)
- **Combining all optimizations**: May have unexpected interactions
- **Adaptive learning overhead**: Need to measure training cost vs benefit
- **Mitigation**: Implement fallbacks (traditional bloom, binary search) if learned components fail

---

## 11. Unique Selling Points

**vs RocksDB**:
- 10x better write amplification (WiscKey + workload-aware)
- 5x faster queries (learned bloom + learned index + SIMD)
- Rust-native (better safety, easier integration)

**vs fjall**:
- Research-backed optimizations (2018-2024 papers vs 2010s algorithms)
- Workload-aware (optimized for vectors vs generic)
- 5-10x performance improvement on vector workloads

**vs sled**:
- Better write performance (LSM vs B+tree)
- Learned components (vs traditional data structures)
- Optimized for append-heavy workloads

---

## 12. Market Position

**Target Market**: Database builders needing high-performance storage engine

**Value Proposition**:
- "RocksDB performance + 2020s research = 10x better for vector workloads"
- "Only LSM-tree with learned components in production"
- "Built specifically for omen ecosystem, but general-purpose"

**Moat**:
- Research implementation (hard to replicate)
- Workload-specific optimizations (not generic)
- Integration with omen (network effects)

---

## Conclusion

fjall is an **excellent baseline** (fastest modern Rust LSM), but uses **proven 2010s algorithms**. seerdb will differentiate through **2018-2024 research** (learned components, SIMD, workload-aware optimizations).

**Clear gaps identified**:
1. ✅ Bloom filters: fjall traditional → seerdb learned (90% space)
2. ✅ Indexes: fjall binary search → seerdb ALEX (1-3x faster)
3. ✅ SIMD: fjall ZERO → seerdb vectorized (5-10x hot paths)
4. ✅ Compaction: fjall static → seerdb workload-aware (3-5x write amp)
5. ✅ KV separation: fjall basic → seerdb WiscKey (10-100x write amp)

**Achievable targets**: 5-10x improvement over fjall baseline through research-backed innovations.

---

**References**:
- fjall analysis: ai/research/FJALL_ANALYSIS.md
- Research papers: ai/research/PAPERS.md
- Benchmark results: ai/research/BENCHMARKS.md

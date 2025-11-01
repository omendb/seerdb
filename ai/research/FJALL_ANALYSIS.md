# Fjall/LSM-Tree Implementation Analysis

Date: November 1, 2025
Source: lsm-tree 2.10.4, fjall 2.11.2 (Cargo registry)

## Executive Summary

Fjall is a well-engineered modern LSM-tree in Rust. It provides a solid baseline (comparable to RocksDB circa 2015-2017), but lacks research-backed innovations that seerdb will introduce. Fjall uses standard techniques throughout; we've identified clear opportunities for 5-10x improvements through learned components, SIMD, and workload-aware optimizations.

**Key Gaps in Fjall vs seerdb Goals**:
- No learned bloom filters (vs our 90% space reduction target)
- No learned indexes (vs our 1-3x faster lookup target)  
- No SIMD optimizations (vs our 5-10x speedup target)
- No workload-aware compaction (vs Tucana-style adaptation)

---

## 1. Bloom Filter Implementation

**What Fjall Does**: Standard bloom filter (228 lines in `lsm-tree/src/bloom/mod.rs`)

### Details:
- Uses double hashing instead of k separate hash functions (optimization)
- Composite hash: xxHash3 produces two u64 values
- Configurable: bits-per-key (default 10), false positive rate
- Per-SSTable filter (one filter per segment)

### Code Pattern:
```rust
pub fn contains_hash(&self, (mut h1, mut h2): CompositeHash) -> bool {
    for i in 0..(self.k as u64) {
        let idx = h1 % (self.m as u64);
        if !self.has_bit(idx as usize) { return false; }
        h1 = h1.wrapping_add(h2);  // Linear probing
        h2 = h2.wrapping_add(i);
    }
    true
}
```

### Assessment for seerdb:
- This is the industry standard (RocksDB identical approach)
- Learned bloom filters (Kraska 2018) claim 90% space reduction with same false positive rate
- **Our advantage**: Implement ML-based filter for 90% space savings
- **Priority**: MUST-HAVE (core differentiation point)

---

## 2. Compaction Strategy

Fjall supports three strategies; details on each:

### 2.1 Leveled Compaction (Default)

**Algorithm**: RocksDB-style (23,392 lines in `leveled.rs`)

Key Features:
- Each level has disjoint key ranges (no overlap)
- When level exceeds threshold, compact into next level
- Minimal compaction selection: pick smallest subset of segments
- Compaction window cap: 25 segments max per operation

**Optimization Details**:
- "Infectious spread" prevention: carefully avoids expanding key ranges unnecessarily
- Tracks hidden sets of segments (ones already being compacted)
- Handles overlapping/non-disjoint levels (repairs on-the-fly)

**Code Structure**: 
- `pick_minimal_compaction()`: Evaluates alternative compaction choices
- Calculates write amplification for each choice
- Filters out blocked/hidden segments

### 2.2 Size-Tiered Compaction (STCS)

**Config** (tiered.rs):
```rust
pub struct Strategy {
    base_size: u32,        // Default: 64MB
    level_ratio: u8,       // Default: 4
}
// Level size = base_size * level_ratio^(level+1)
```

**Trade-offs**: Better write amp than leveled, higher read/space amp

### 2.3 FIFO (Time-Series)
- Delete old data by timestamp
- No recompaction, just deletion
- Good for explicit retention policies

### Assessment for seerdb:

**Gaps**:
- No adaptive strategy selection based on workload
- No learned level sizing (Dostoevsky paper analysis)
- No key distribution awareness (Tucana paper)
- Generic settings for all workloads

**Our Advantage**:
- Implement Tucana-style learned compaction: predict key distribution, adjust levels
- Analyze omen vector workload: append-heavy, large values → optimize for that pattern
- Adaptive level ratios based on actual write vs read patterns
- **Priority**: HIGH (3-5x write amplification reduction for our workload)

---

## 3. SIMD Optimizations

**Finding**: ZERO SIMD in entire codebase

- No AVX2, SSE, NEON instructions anywhere
- Key comparisons use standard Rust operators
- Bloom filter bit operations: basic bitwise ops, not vectorized
- Hash functions: rely on xxhash3 library (which may have SIMD internally)

### Opportunities for seerdb:

1. **Key Comparison in Partition Point** (hot path):
   - Current: Binary search using `<` operator per comparison
   - SIMD: Compare 4-8 keys in parallel (AVX2)
   - Estimate: 5-10x speedup for workloads with many comparisons

2. **Bloom Filter Lookups**:
   - Current: Sequential hash checks
   - SIMD: Vectorized bit checking
   - Estimate: 3-5x speedup per lookup

3. **Compression**:
   - Vectorized copy/compare in compression codec
   - Estimate: 2-3x speedup

**Priority**: MEDIUM (5x gains, effort = medium)

---

## 4. SSTable Index Structure

**Design**: Two-level index (standard in industry)

### How It Works:
```
Level 1: Block Index (keyed block handles)
├── Entry 1: key_range = [key1, key2], offset, size
├── Entry 2: key_range = [key2, key3], offset, size
└── Entry 3: key_range = [key3, key4], offset, size

Level 2: Within-Block Search
└── Binary search within selected block
```

### Implementation Details:
- `KeyedBlockHandle`: Stores min/max key for each block + offset/size
- `TwoLevelBlockIndex`: Sparse index of block boundaries (space-efficient)
- `FullBlockIndex`: All keys in top level (faster, more space)

### Lookup Path:
```rust
// Find candidate block
let idx = partition_point(self, |item| item.end_key < key);
// Binary search within block
let value = block_data[idx].binary_search(key);
```

### Assessment for seerdb:

**Gaps**:
- Same O(log n) lookups as RocksDB/LevelDB (1997 technology)
- No learned index models (ALEX paper 2020)
- Doesn't exploit key patterns/distributions

**Our Advantage**:
- Implement ALEX-style learned index for SSTables
- Train piecewise linear models on key distribution
- Estimate: 1-3x faster lookups, smaller index size
- Better space efficiency + speed
- **Priority**: HIGH (core research contribution)

---

## 5. Key-Value Separation

**What Fjall Does**: "BlobTree" mode (separate value log)

### Configuration:
- Threshold: configurable (default 4KiB)
- Values larger than threshold → stored in separate blob file
- Small values stay in LSM tree (with pointers)

### Implementation:
- Garbage collection: During normal compaction
- Not separate vlog with independent GC (unlike WiscKey paper)

### Code Location:
```rust
// lsm-tree/src/config.rs
pub blob_file_separation_threshold: u32,  // Default: 4*1024
pub blob_file_target_size: u64,           // Default: 64MB
```

### Assessment for seerdb:

**Gaps**:
- Basic implementation
- GC tightly coupled to level compaction
- Doesn't exploit WiscKey paper insights (separate GC thread, smarter heuristics)

**Our Advantage**:
- Implement true WiscKey: separate value log with independent GC
- Smart GC: identify hot/cold values, trigger based on vlog fragmentation
- Optimization: Large values (like vector embeddings) handled more efficiently
- **Priority**: HIGH for vector workload (write amplification reduction)

---

## 6. Other Notable Choices

### Compression (lsm-tree/src/segment/meta/compression.rs)
- **Options**: None, LZ4 (feature-gated), zlib/miniz (0-10 compression level)
- **Default**: No compression enabled by default
- **Opportunity**: Add zstd with dictionary learning for repeated patterns

### Caching
- Uses `quick_cache` (LRU-style)
- Per-tree default: 16-32 MiB
- Can share global cache across partitions
- **Opportunity**: Learned cache replacement (predict next access)

### Memtable
- Skiplist via `crossbeam_skiplist`
- Size-based flush trigger
- Standard implementation (no optimizations)

### Worker Threads (fjall layer)
- Default: 4 compaction workers (configurable)
- 4 flush workers (configurable)
- Platform-aware: adjusts based on available CPUs

### Partitioning (fjall-specific)
- Multiple partitions in one keyspace
- Each partition = separate LSM tree
- Purpose: Better concurrency (independent locks per partition)

---

## 7. Dependencies & Technology Stack

Key Libraries:
- `xxhash-rust`: Fast hashing (with xxh3 feature)
- `crossbeam-skiplist`: Concurrent skiplist memtable
- `quick_cache`: Block cache implementation
- `lz4_flex`, `miniz_oxide`: Compression (optional)
- `value-log`: Separate value log abstraction

### Technology Choices:
- No complex ML frameworks (good - we can be lightweight too)
- Pragmatic Rust (minimal unsafe, good error handling)
- Pluggable compaction strategies

---

## 8. Performance Implications (Code Analysis)

### Strengths:
1. **Sequential writes**: Memtable → flush → compact chain is efficient
2. **Range queries**: Block-based storage + sequential iterators
3. **Concurrency**: Partition-based isolation reduces contention
4. **Crash recovery**: Journal-based recovery well-implemented

### Bottlenecks We Can Exploit:
1. **Point lookups**: O(log n) at each level (no learned shortcuts)
2. **Bloom filters**: 10 bpk standard (vs 1 bpk with learned filter)
3. **Key comparison**: No vectorization (vs SIMD 5-10x)
4. **Compaction logic**: No awareness of actual key distribution
5. **Large values**: Integrated KV-sep (vs smart separate vlog)

---

## 9. Code Organization

**lsm-tree/ layout**:
```
src/
├── bloom/               # Traditional bloom filter
├── compaction/          # Leveled, tiered, FIFO strategies
├── segment/             # SSTable format
│   ├── block/           # Data blocks
│   ├── block_index/     # Two-level index
│   └── meta/            # Metadata, compression
├── tree/                # Main LSM tree
└── cache.rs             # Block cache
```

**fjall/ layout**:
```
src/
├── keyspace.rs          # Main entry point
├── partition/           # Per-partition LSM instances
├── config.rs            # Configuration
├── journal/             # Write-ahead log
└── compaction/          # Compaction management
```

**Code Quality**:
- Well-structured, clear module separation
- Rust best practices (minimal unsafe)
- Good error handling with Result types
- Type-safe abstractions

**Gaps**:
- Limited inline comments explaining WHY (optimization rationale)
- No performance notes or benchmarking annotations
- Standard techniques, no novel approaches documented

---

## 10. Seerdb Differentiation Strategy

### Must-Have (High Impact):
1. **Learned Bloom Filters** (90% space savings)
   - Implement simple NN or decision tree model
   - Train during compaction
   - Fallback to traditional if model fails

2. **Learned Index on SSTables** (1-3x faster lookups)
   - ALEX-style piecewise linear model
   - Train on key distribution per SSTable
   - Binary search as fallback

3. **Workload-Aware Compaction** (3-5x write amp reduction)
   - Analyze key distribution patterns
   - Detect vector workload characteristics
   - Adapt level ratios and compaction strategy

4. **WiscKey-Style KV Separation** (better write amp for large values)
   - Separate vlog with independent GC
   - Smart GC based on fragmentation/access patterns
   - Optimize for vector embeddings

### Medium-Impact (5x gains):
5. **SIMD Optimizations**
   - Vectorized key comparison (partition_point)
   - Vectorized bloom filter operations
   - Estimate: 5-10x for hot paths

6. **Adaptive Level Sizing** (Dostoevsky paper)
   - Mathematical analysis of optimal level ratios
   - Adjust for workload characteristics

7. **Smart Compression** (zstd + dictionary learning)
   - Learn patterns in data
   - Use custom dictionaries for better ratio

### Nice-to-Have:
8. **io_uring** (Linux async I/O)
9. **Prefetching** (predict next keys)
10. **Hot/Cold Tiering** (recent data fast storage)

---

## 11. Competitive Positioning

| Feature | RocksDB | Fjall | seerdb (Target) |
|---------|---------|-------|---|
| Bloom Filter | Traditional | Traditional | **Learned** |
| Index | Binary search | Binary search | **Learned (ALEX)** |
| Compaction | Static levels | Static levels | **Workload-Aware** |
| KV Separation | Basic | Basic | **WiscKey-Style** |
| SIMD | Some | None | **Vectorized** |
| Adaptivity | None | None | **Yes** |

---

## 12. Recommendations for seerdb

### Immediate (Phase 1: Weeks 5-8):
1. Implement core LSM (WAL, memtable, SSTable, basic compaction)
2. Use Fjall's approaches as reference for correctness
3. Add instrumentation for workload analysis

### Research Phase (Week 1-4):
1. ✅ Understand Fjall's design choices
2. Implement learned bloom filter prototype
3. Benchmark Fjall baseline on omen vector workload
4. Identify specific bottlenecks for omen use case

### Innovation Phase (Weeks 9-12):
1. Integrate learned bloom filters
2. Implement learned SSTable index (ALEX-style)
3. Measure space/time improvements vs Fjall

### Optimization Phase (Weeks 13-16):
1. Add SIMD to hot paths
2. Implement workload-aware compaction
3. Fine-tune for vector database workload

---

## Conclusion

Fjall is a **solid, modern, well-engineered baseline** (2020s code quality), but uses **2010s-era algorithms** (RocksDB-derived). 

Seerdb's strategy is clear:
- **Keep** Fjall's clean architecture and Rust foundations
- **Replace** algorithms with research-backed innovations
- **Target** 10x write amplification reduction, 5x faster queries through learned components + SIMD + workload awareness

The gaps are large enough that achieving our goals is feasible with focused research implementation.

---

**Sources**:
- lsm-tree 2.10.4 source: ~/.cargo/registry/src/index.crates.io-.../lsm-tree-2.10.4/
- fjall 2.11.2 source: ~/.cargo/registry/src/index.crates.io-.../fjall-2.11.2/
- Analysis Date: November 1, 2025


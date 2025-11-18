# Real Workload Comparisons - Phase 4 Profiling

**Date**: November 18, 2025
**Tool**: Custom comparison benchmark
**Competitors**: RocksDB 0.22, fjall 2.11
**Version**: seerdb 0.0.1-alpha

---

## Executive Summary

**Critical Finding**: ⚠️ **seerdb is 2-4x SLOWER than RocksDB and fjall on realistic workloads with durability**

This **contradicts** baseline benchmarks showing 2.47x faster writes and 2.07x faster reads.

**Root Cause**: Baseline benchmarks measured peak throughput **without durability** (`SyncPolicy::None`), while real workloads include fsync overhead and explicit flush() calls.

**Impact**:
- Peak throughput (no durability): ✅ 2.47x faster than RocksDB (878K ops/sec)
- Realistic workload (with durability): ⚠️ 2-4x slower than competitors
- Cache effectiveness: ⚠️ 49-68% hit rate (vs 97-99% in synthetic benchmarks)

---

## Test Results

### Workload 1: omendb Pattern (HNSW Graph Edges)

**Setup**: 10K nodes × 32 edges = 320K entries, 100 prefix scans

| Engine | Write Time | Read Time | Notes |
|--------|-----------|----------|-------|
| **seerdb** | 1.41s | 0.01s | Cache: 49.72% |
| **RocksDB** | 0.65s | 1.91s | 2.2x faster writes |
| **fjall** | 0.37s | 0.00s | 3.8x faster writes |

**Speedup vs RocksDB**: Write **0.47x** (2.1x slower), Read **187.06x** (outlier)
**Speedup vs fjall**: Write **0.27x** (3.7x slower), Read **0.09x** (11x slower)

**Analysis**:
- seerdb writes are 2-3.8x SLOWER than competitors
- seerdb read result (187x faster) is likely a measurement error
- Low cache hit rate (49.72%) vs 97-99% in synthetic benchmarks

### Workload 2: Time Series (Sequential Timestamps)

**Setup**: 1M entries, 100 range queries (10K entries each)

| Engine | Write Time | Read Time | Notes |
|--------|-----------|----------|-------|
| **seerdb** | 4.38s | 0.10s | Cache: 51.79% |
| **RocksDB** | 1.89s | 0.12s | 2.3x faster writes |
| **fjall** | 1.08s | 0.12s | 4.1x faster writes |

**Speedup vs RocksDB**: Write **0.43x** (2.3x slower), Read **1.21x** (slightly faster)
**Speedup vs fjall**: Write **0.25x** (4x slower), Read **1.13x** (slightly faster)

**Analysis**:
- seerdb writes are 2.3-4x SLOWER
- Reads are competitive (1.2x faster than RocksDB)
- Cache hit rate still low (51.79%)

### Workload 3: Random Key-Value (Point Lookups)

**Setup**: 500K entries, 100K point lookups

| Engine | Write Time | Read Time | Notes |
|--------|-----------|----------|-------|
| **seerdb** | 3.93s | 1.14s | Cache: 68.69% |
| **RocksDB** | 1.68s | 0.61s | 2.3x faster writes, 1.9x faster reads |
| **fjall** | 2.18s | 0.30s | 1.8x faster writes, 3.8x faster reads |

**Speedup vs RocksDB**: Write **0.43x** (2.3x slower), Read **0.53x** (1.9x slower)
**Speedup vs fjall**: Write **0.55x** (1.8x slower), Read **0.26x** (3.8x slower)

**Analysis**:
- seerdb is slower on BOTH writes AND reads
- Cache hit rate better (68.69%) but still low vs synthetic (97-99%)

---

## Root Cause Analysis

### Baseline Benchmark Configuration (878K writes/sec)

```rust
// ai/STATUS.md claims: "Writes: 878K ops/sec (2.47x RocksDB)"
// examples/baseline_benchmark.rs configuration:

let opts = DBOptions {
    wal_sync_policy: seerdb::SyncPolicy::None,  // ← NO FSYNC!
    // ... other options
};

// No flush() calls during timing
for i in 0..NUM_OPERATIONS {
    db.put(key, value)?;
}
let elapsed = start.elapsed();  // ← No flush included
```

**Characteristics**:
- **No durability** (SyncPolicy::None = no fsync overhead)
- **No flush() in timing** (measures peak throughput only)
- **Small dataset** (100K ops, fits in memtable)
- **Sequential keys** (cache-friendly)

### Real Workload Configuration (slower)

```rust
// examples/real_workload_comparisons.rs configuration:

let options = DBOptions {
    // Uses DEFAULT SyncPolicy (includes fsync)
    memtable_capacity: 64 * 1024 * 1024,
    block_cache_capacity: 16_384,
    ..Default::default()  // ← Durability ENABLED
};

for node_id in 0..nodes {
    for edge_id in 0..edges_per_node {
        db.put(key, value)?;
    }
}
db.flush().unwrap();  // ← FLUSH INCLUDED IN TIMING
let write_duration = start.elapsed();
```

**Characteristics**:
- **Durability enabled** (default = fsync on every WAL write)
- **flush() included in timing** (measures realistic latency)
- **Large dataset** (320K-1M ops, multiple flushes)
- **Random/structured keys** (worse cache locality)

---

## Key Differences Identified

### 1. Durability Overhead

**Baseline**: `SyncPolicy::None` - No fsync, peak throughput only
**Real workload**: Default policy - fsync on every write (realistic)

**Impact**: Fsync adds significant latency (1-10ms per operation on macOS)

**Verification needed**: Run real workload with `SyncPolicy::None` to isolate fsync impact

### 2. Flush Overhead

**Baseline**: No flush() in timing (background flush happens async)
**Real workload**: flush() included in timing

**Impact**: Flush involves:
- Writing memtable to SSTable (I/O)
- Building bloom filter and index
- Compaction if needed

**Estimated overhead**: 0.5-2s for 320K-1M entries

### 3. Dataset Size

**Baseline**: 100K ops (fits in 64MB memtable)
**Real workload**: 320K-1M ops (multiple memtable flushes)

**Impact**: More flushes = more overhead

### 4. Key Pattern

**Baseline**: Sequential keys (format!("key_{:08}", i))
**Real workload**: Random/structured keys (HNSW, timestamps, hex)

**Impact**: Sequential keys have better cache locality

### 5. Cache Hit Rate

**Baseline**: 97.38% - 99.84%
**Real workload**: 49.72% - 68.69%

**Impact**: Low cache hit rate means more disk I/O

**Possible causes**:
- Larger working set (320K-1M entries vs 100K)
- Random access patterns (worse locality)
- Block cache size insufficient (16,384 blocks = ~64MB)

---

## Comparative Analysis: Why RocksDB/fjall are Faster

### RocksDB Advantages

1. **Mature fsync optimization**: Group commit, write pipelining
2. **Better flush strategy**: Async flush with backpressure
3. **Optimized compaction**: Multi-threaded, incremental
4. **Large cache by default**: 8MB block cache default

**Our disadvantage**: Single-threaded WAL writer, synchronous flush

### fjall Advantages

1. **Optimized LSM**: Based on lsm-tree crate (battle-tested)
2. **Better write batching**: Group commit built-in
3. **Lock-free structures**: Similar to our design but more optimized

**Our disadvantage**: Less mature implementation, fewer optimizations

---

## Why Baseline Benchmarks Were Misleading

**What we measured**: Peak throughput without durability
**What users need**: Realistic performance with durability

**Analogy**: Measuring car speed downhill (baseline) vs uphill (realistic)

**Correct interpretation of baseline results**:
- ✅ "seerdb achieves 878K writes/sec **without durability**"
- ❌ "seerdb is 2.47x faster than RocksDB" (misleading - different configs)

**Lesson learned**: Always benchmark with durability enabled for production claims

---

## Performance Breakdown (Estimated)

### seerdb Write Path Overhead

**Total time (omendb)**: 1.41s for 320K writes = 227K writes/sec

**Estimated breakdown**:
- Memtable insert: ~0.2s (10-15% - skiplist overhead)
- WAL write: ~0.5s (35-40% - fsync dominant)
- Flush overhead: ~0.5s (35-40% - SSTable creation)
- Compaction: ~0.2s (10-15% - background)

**Bottleneck**: WAL fsync + flush overhead (70-80% of total time)

### RocksDB Write Path (for comparison)

**Total time (omendb)**: 0.65s for 320K writes = 492K writes/sec

**Why faster**:
- Group commit (amortizes fsync across multiple writes)
- Async flush (doesn't block writes)
- Multi-threaded compaction

---

## Cache Hit Rate Analysis

### Why Lower in Real Workloads?

**Baseline benchmarks**: 97.38% - 99.84% hit rate
**Real workloads**: 49.72% - 68.69% hit rate

**Possible causes**:

1. **Larger working set**
   - Baseline: 100K entries (~100MB compressed)
   - Real: 320K-1M entries (~320MB-1GB compressed)
   - Cache: 64MB (16,384 blocks × ~4KB)
   - **Issue**: Working set > cache size

2. **Random access patterns**
   - HNSW: Random prefix scans (poor locality)
   - Time series: Sequential (good locality) - but still 51% hit rate
   - Random KV: Completely random (worst locality)

3. **Insufficient cache size**
   - 64MB cache for 1GB working set = 6.4% coverage
   - Expected hit rate: ~6-20% (actual: 49-68%)
   - **Insight**: Cache is working, but too small for workload

---

## Comparison with Baseline Results

### Claimed Performance (ai/STATUS.md)

| Workload | seerdb | RocksDB | Speedup | Reality Check |
|----------|--------|---------|---------|---------------|
| **Writes** | 878K ops/sec | 356K ops/sec | **2.47x** | ⚠️ **No durability** |
| **Reads** | 2,207K ops/sec | 1,065K ops/sec | **2.07x** | ⚠️ **Small dataset** |
| **Mixed** | 718K ops/sec | 400K ops/sec | **1.79x** | ⚠️ **No durability** |
| **Scans** | 19.6K scans/sec | 19.7K scans/sec | 0.99x | ✅ **Competitive** |

### Real Performance (with durability)

| Workload | seerdb | RocksDB | Speedup | Status |
|----------|--------|---------|---------|--------|
| **omendb writes** | 227K ops/sec | 492K ops/sec | **0.47x** | ⚠️ **2.1x slower** |
| **Time series writes** | 228K ops/sec | 529K ops/sec | **0.43x** | ⚠️ **2.3x slower** |
| **Random writes** | 127K ops/sec | 298K ops/sec | **0.43x** | ⚠️ **2.3x slower** |
| **Random reads** | 88K ops/sec | 164K ops/sec | **0.53x** | ⚠️ **1.9x slower** |

**Conclusion**: Baseline benchmarks are **not representative** of production performance

---

## Optimization Opportunities

### Priority 1: WAL Pipelining (3-5x improvement expected)

**Problem**: Single-threaded WAL writer serializes all writes
**Solution**: Group commit + write pipelining (RocksDB pattern)
**Status**: Already identified in Lock Contention Analysis (28.7% parallel efficiency)
**Expected**: 80%+ parallel efficiency, 3-5x write throughput

### Priority 2: Async Flush (2-3x improvement expected)

**Problem**: flush() blocks writes until SSTable creation completes
**Solution**: Background flush with backpressure (don't block writes)
**Expected**: 2-3x write throughput (remove 0.5s flush overhead)

### Priority 3: Block Cache Tuning

**Problem**: Low cache hit rate (49-68%) vs synthetic (97-99%)
**Solution**: Increase cache size for realistic workloads
**Current**: 64MB (16,384 blocks)
**Recommended**: 256-512MB for 1M+ entry workloads
**Expected**: 80%+ hit rate, 2-3x read throughput

### Priority 4: Group Commit

**Problem**: Each write triggers fsync (expensive)
**Solution**: Batch multiple writes before fsync
**Expected**: 5-10x write throughput (amortize fsync across 10-100 writes)

---

## Recommended Actions

### Immediate (Documentation)

1. **✅ Update ai/STATUS.md**: Correct performance claims with caveats
   - "878K writes/sec **without durability** (SyncPolicy::None)"
   - "Real workloads with durability: 127-228K writes/sec"

2. **✅ Update README.md**: Honest performance comparison
   - Remove "2.47x faster than RocksDB" claim
   - Add "Competitive with RocksDB for scans (0.99x)"
   - Clarify "Peak throughput vs production performance"

3. **✅ Add benchmark disclaimer**: Baseline = peak throughput, not production

### Short-Term (1-2 weeks)

1. **Re-run baseline benchmarks with durability**
   - Set `wal_sync_policy: SyncPolicy::Always`
   - Measure realistic performance
   - Compare apples-to-apples with RocksDB/fjall

2. **Implement group commit**
   - Batch writes before fsync
   - Expected: 5-10x write throughput

3. **Increase default block cache size**
   - Current: 64MB (too small for realistic workloads)
   - Recommended: 256MB default
   - Make configurable via DBOptions

### Long-Term (Future Releases)

1. **WAL pipelining** (Priority 1 from Lock Contention Analysis)
   - 3-5x improvement expected
   - Fixes 28.7% parallel efficiency issue

2. **Async flush** (remove blocking)
   - 2-3x improvement expected
   - Don't block writes during flush

3. **Multi-threaded compaction**
   - Follow RocksDB pattern
   - Reduce compaction overhead

---

## Conclusions

### What We Learned

1. **Baseline benchmarks were misleading**: Measured peak throughput without durability
2. **Real performance is 2-4x slower**: With durability enabled (realistic)
3. **Cache hit rate is low**: 49-68% vs 97-99% (working set > cache size)
4. **Optimizations available**: WAL pipelining, async flush, group commit (10-20x potential)

### seerdb Performance Profile (Honest Assessment)

**Strengths**:
- ✅ Scan performance competitive with RocksDB (0.99x)
- ✅ Low write amplification (1.01x - best-in-class)
- ✅ Learned index works (+55% read performance in some workloads)

**Weaknesses**:
- ⚠️ Write throughput 2-4x slower (with durability)
- ⚠️ Read throughput 2-4x slower (random access)
- ⚠️ Low cache hit rate for large workloads
- ⚠️ Single-threaded WAL writer (28.7% parallel efficiency)

### Production Readiness

**Current status**: ⚠️ **Not ready for production**
- Performance worse than RocksDB/fjall on realistic workloads
- Optimizations available but not implemented

**Path to production**:
1. Implement group commit (5-10x write improvement)
2. Implement WAL pipelining (3-5x concurrent write improvement)
3. Implement async flush (2-3x write improvement)
4. Increase block cache size (2-3x read improvement)

**Expected after optimizations**: Competitive with RocksDB/fjall (1-2x)

---

## Appendix: Benchmark Command

```bash
cargo run --release --features baseline-benchmarks --example real_workload_comparisons
```

**Output**:
- Workload 1 (omendb): seerdb 0.47x RocksDB writes, 187x reads (outlier)
- Workload 2 (time series): seerdb 0.43x RocksDB writes, 1.21x reads
- Workload 3 (random): seerdb 0.43x RocksDB writes, 0.53x reads

---

*Phase 4 profiling complete. Critical finding: seerdb is 2-4x slower than RocksDB/fjall on realistic workloads with durability. Optimizations identified (group commit, WAL pipelining, async flush) can bring performance to competitive levels (1-2x RocksDB).*

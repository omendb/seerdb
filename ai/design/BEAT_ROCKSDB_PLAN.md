# Plan to Beat RocksDB

**Created**: November 7, 2025
**Goal**: Match or exceed RocksDB performance in all workloads while maintaining best-in-class write amplification

---

## Current Reality (After SSTable Filtering)

| Workload | seerdb | RocksDB | Ratio | Gap (ops/sec) |
|----------|--------|---------|-------|---------------|
| **Writes** | 218K | 356K | 0.61x | **-138K (39%)** 🔴 |
| **Reads** | 872K | 1,070K | 0.81x | **-198K (19%)** ⚠️ |
| **Mixed** | 311K | 407K | 0.76x | **-96K (24%)** ⚠️ |
| **Scans** | 17,087 | 21,175 | 0.81x | **-4,088 (19%)** ⚠️ |
| **Write Amp** | 1.01x | 4.88x | **4.82x better** | ✅ **BEST** |

**Strengths**:
- Write amplification: 4.82x better than RocksDB (1.01x vs 4.88x)
- Learned components: Only Rust LSM with ALEX + learned blooms
- Safe Rust: No unsafe code

**Weaknesses**:
- Write throughput: 39% slower
- Read throughput: 19% slower
- Range scans: 19% slower

---

## Analysis: Where RocksDB Beats Us

### 1. Writes (39% slower) - BIGGEST GAP

**RocksDB**: 356K ops/sec
**seerdb**: 218K ops/sec
**Gap**: 138K ops/sec

**Known Bottlenecks** (from previous profiling):
- WAL write time: 48.5% of total time (even without fsync!)
- WAL uses std::fs (blocking I/O, syscalls)
- Memtable insert: ~30% of time
- Lock contention: Unknown (needs profiling)

**Root Causes**:
1. **WAL I/O**: std::fs blocking writes, one syscall per write
2. **Memtable**: Possible lock contention (crossbeam-skiplist uses locks)
3. **Allocations**: BytesMut allocations on every write
4. **Batching**: WAL batch size may be suboptimal

**Optimization Opportunities**:
- Tokio async I/O: Non-blocking async I/O (+30-50% potential, safer than io_uring)
- WAL batching: More aggressive batching (+20-30%)
- Lock-free memtable: Reduce contention (+10-20%)
- Allocation pooling: Reuse BytesMut (+5-10%)

### 2. Reads (19% slower)

**RocksDB**: 1,070K ops/sec
**seerdb**: 872K ops/sec
**Gap**: 198K ops/sec

**Potential Bottlenecks**:
- Bloom filter: Traditional vs learned (learned may be slower for queries)
- ALEX index: May have overhead vs binary search
- Block cache: LRU may not be optimal
- Decompression: Snappy decompression on every block miss

**Optimization Opportunities**:
- Blocked bloom filters: 3x faster (from research) (+30-50%)
- ALEX tuning: Optimize model complexity (+10-20%)
- Block cache policy: LFU or adaptive replacement (+5-10%)
- Prefetching: Speculatively load adjacent blocks (+10-15%)

### 3. Range Scans (19% slower)

**RocksDB**: 21,175 scans/sec
**seerdb**: 17,087 scans/sec
**Gap**: 4,088 scans/sec

**Potential Bottlenecks**:
- K-way merge overhead: BinaryHeap operations
- Iterator allocation: Creating iterators has overhead
- Block loading: No readahead for sequential access
- Key comparisons: Standard comparison (no SIMD)

**Optimization Opportunities**:
- Adaptive readahead: Prefetch next blocks (+30-50%)
- SIMD key comparisons: Vectorized compare in merge (+10-20%)
- Iterator pooling: Reuse iterators (+5-10%)
- Block cache warming: Keep hot blocks in cache (+10-15%)

---

## Phase-by-Phase Optimization Plan

### Phase 8: Close the Gap (1-2 weeks)

**Goal**: Reach 0.9x+ RocksDB across all workloads

#### Task 1: Write Path Profiling and Optimization (3-4 days)

**Profile**:
```bash
cargo install flamegraph
cargo flamegraph --example write_benchmark
```

**Expected findings**:
- WAL write: 40-50% of time
- Memtable insert: 25-35% of time
- Lock contention: 10-20% of time
- Allocations: 5-10% of time

**Optimizations to implement** (priority order):
1. **WAL Batching Tuning** (1 day)
   - Increase batch size: 8MB → 16MB
   - Reduce flush interval: 100ms → 50ms
   - Expected: +10-15% write throughput

2. **Memtable Optimization** (1 day)
   - Pre-allocate capacity in skiplist
   - Check for lock contention hotspots
   - Expected: +5-10% write throughput

3. **Allocation Pooling** (1 day)
   - Pool BytesMut for WAL records
   - Reuse buffers across writes
   - Expected: +5-10% write throughput

**Target**: 270K+ ops/sec (0.76x → 0.76x+ RocksDB)
**Total impact**: +20-30% write throughput

#### Task 2: Read Path Profiling and Optimization (2-3 days)

**Profile**:
```bash
cargo flamegraph --example read_benchmark
```

**Expected findings**:
- Bloom filter: 15-25% of time
- Block cache lookup: 10-15% of time
- ALEX index: 10-15% of time
- Block decompression: 20-30% of time

**Optimizations to implement**:
1. **Blocked Bloom Filters** (1 day)
   - Implement cache-friendly blocked bloom (from research)
   - 64-byte blocks aligned to cache lines
   - Expected: +20-30% bloom filter speed → +10-15% read throughput

2. **Block Cache Tuning** (1 day)
   - Increase default size: 8MB → 32MB
   - Consider LFU policy instead of LRU
   - Expected: +10-15% cache hit rate → +5-10% read throughput

3. **ALEX Optimization** (1 day)
   - Profile ALEX lookup vs binary search
   - Tune model complexity for 1M key SSTables
   - Expected: +5-10% read throughput

**Target**: 1,020K+ ops/sec (0.81x → 0.95x RocksDB)
**Total impact**: +15-25% read throughput

#### Task 3: Range Scan Optimization (1-2 days)

**Profile**:
```bash
cargo flamegraph --example range_benchmark
```

**Optimizations to implement**:
1. **Adaptive Readahead** (1 day)
   - Detect sequential access pattern
   - Prefetch next 2-4 blocks in background
   - Expected: +25-40% scan throughput

2. **Block Cache Warming** (0.5 day)
   - Keep recently scanned blocks in cache
   - Increase priority for scan blocks
   - Expected: +5-10% scan throughput

**Target**: 20,000+ scans/sec (0.81x → 0.94x RocksDB)
**Total impact**: +17-30% scan throughput

---

### Phase 9: Match RocksDB (1-2 weeks)

**Goal**: Reach 1.0x RocksDB in all workloads

#### Task 1: Tokio Async I/O Integration (3-4 days)

**Decision**: Use tokio::fs instead of io_uring
- ✅ Security: Pure Rust, no kernel CVEs (io_uring has several)
- ✅ Cross-platform: Works on macOS (dev) + Linux (prod)
- ✅ Safety: Battle-tested, no unsafe kernel interface
- ⚠️ Performance: 20-30% less than io_uring, but acceptable trade-off

**Implementation**:
- Replace std::fs with tokio::fs (async I/O)
- Non-blocking WAL writes (async/await)
- Batch multiple writes efficiently
- Async SSTable reads during compaction

**Expected Impact**:
- WAL writes: +30-50% (non-blocking I/O)
- Compaction: +50-100% (async batch reads)
- Overall writes: +30-40% (WAL is 48.5% of time)

**Target**: 300K+ write ops/sec (0.84x RocksDB)

#### Task 2: SIMD Optimizations (2-3 days)

**Implementation**:
1. **SIMD Key Comparisons** (1 day)
   - Vectorize memcmp in k-way merge
   - Use AVX2 on x86, NEON on ARM
   - Expected: +20-30% merge speed → +15-20% scan throughput

2. **SIMD Bloom Filters** (1 day)
   - Parallel hash computation
   - Vectorized bit checks
   - Expected: +30-50% bloom filter speed → +10-15% read throughput

**Target**:
- Reads: 1,020K+ ops/sec (0.95x RocksDB)
- Scans: 24,000+ scans/sec (1.13x RocksDB!)

#### Task 3: Lock-Free Memtable (3-4 days)

**Implementation**:
- Replace crossbeam-skiplist with custom lock-free implementation
- Use atomic operations for concurrent inserts
- Reduce contention on write path

**Expected Impact**: +15-25% write throughput

**Target**: 360K+ write ops/sec (1.01x RocksDB!)

---

### Phase 10: Beat RocksDB (2-3 weeks)

**Goal**: Exceed RocksDB in key workloads (1.2x+ in specific areas)

#### Task 1: Workload-Aware Auto-Tuning (1 week)

**Implementation** (CAMAL-inspired):
- Detect workload characteristics:
  - Key sortedness (sorted → use tiered compaction)
  - Read/write ratio (read-heavy → larger bloom filters)
  - Value size distribution (large values → aggressive vLog)
- Auto-tune parameters:
  - Compaction strategy
  - Bloom filter size
  - vLog threshold
  - Memtable size

**Expected Impact**: +20-30% on optimized workloads

**Target**:
- Write-heavy: 400K+ ops/sec (1.12x RocksDB)
- Read-heavy: 1,200K+ ops/sec (1.12x RocksDB)

#### Task 2: Advanced Bloom Filters (1 week)

**Implementation**:
- Learned bloom filters (neural network or decision tree)
- 90% space reduction (from research)
- Adaptive model complexity based on SSTable size

**Expected Impact**:
- Space: 90% reduction (claim from paper)
- Reads: +10-20% (less cache pressure from smaller blooms)

#### Task 3: Advanced Caching (1 week)

**Implementation**:
- Read hotness tracking (track access frequency per key)
- Optimize ALEX index for hot keys (more complex models)
- Adaptive block cache (learn access patterns)

**Expected Impact**: +15-25% on read-heavy workloads

**Target**: 1,300K+ read ops/sec (1.21x RocksDB!)

---

## Timeline Summary

| Phase | Duration | Goal | Key Optimizations |
|-------|----------|------|-------------------|
| **Phase 8** | 1-2 weeks | 0.9x RocksDB | WAL batching, blocked blooms, readahead |
| **Phase 9** | 1-2 weeks | 1.0x RocksDB | Tokio async I/O, SIMD, lock-free memtable |
| **Phase 10** | 2-3 weeks | 1.2x RocksDB | Workload-aware, learned blooms, advanced caching |

**Total**: 4-7 weeks to beat RocksDB

---

## Expected Final Performance

| Workload | Current | Phase 8 | Phase 9 | Phase 10 | Target |
|----------|---------|---------|---------|----------|--------|
| **Writes** | 218K (0.61x) | 270K (0.76x) | 360K (1.01x) | 400K (1.12x) | **1.12x RocksDB** |
| **Reads** | 872K (0.81x) | 1,020K (0.95x) | 1,020K (0.95x) | 1,300K (1.21x) | **1.21x RocksDB** |
| **Mixed** | 311K (0.76x) | 360K (0.88x) | 400K (0.98x) | 450K (1.11x) | **1.11x RocksDB** |
| **Scans** | 17,087 (0.81x) | 20,000 (0.94x) | 24,000 (1.13x) | 27,000 (1.27x) | **1.27x RocksDB** |
| **Write Amp** | 1.01x | 1.01x | 1.01x | 1.01x | **4.82x better** ✅ |

**Key Achievements**:
- ✅ Beat RocksDB in writes (1.12x)
- ✅ Beat RocksDB in reads (1.21x)
- ✅ Beat RocksDB in mixed (1.11x)
- ✅ Beat RocksDB in scans (1.27x)
- ✅ Maintain best-in-class write amplification (4.82x better)

---

## Risk Assessment

### Low Risk (90%+ confidence)
- WAL batching tuning: Proven approach, easy to implement
- Blocked bloom filters: Well-researched, clear implementation path
- Adaptive readahead: Standard optimization, used by RocksDB

### Medium Risk (70-80% confidence)
- Tokio async I/O: Requires async/await rewrite, but battle-tested library
- SIMD: Platform-specific, needs fallback for non-SIMD CPUs
- Lock-free memtable: Complex, potential subtle bugs

### High Risk (50-60% confidence)
- Workload-aware tuning: Complex ML, may not generalize well
- Learned bloom filters: Model training overhead, may be slower than traditional
- Advanced caching: Complex heuristics, may not always help

**Mitigation**:
- Start with low-risk optimizations (Phase 8)
- Validate each optimization with benchmarks
- Keep rollback path for failed optimizations
- Don't regress write amplification (best-in-class)

---

## Success Metrics

### Phase 8 Success
- ✅ Writes: 270K+ ops/sec (0.76x RocksDB)
- ✅ Reads: 1,020K+ ops/sec (0.95x RocksDB)
- ✅ Scans: 20,000+ scans/sec (0.94x RocksDB)
- ✅ Mixed: 360K+ ops/sec (0.88x RocksDB)

### Phase 9 Success
- ✅ Writes: 360K+ ops/sec (1.01x RocksDB)
- ✅ Reads: 1,020K+ ops/sec (0.95x RocksDB)
- ✅ Scans: 24,000+ scans/sec (1.13x RocksDB)
- ✅ Mixed: 400K+ ops/sec (0.98x RocksDB)

### Phase 10 Success (Stretch Goals)
- ✅ Writes: 400K+ ops/sec (1.12x RocksDB)
- ✅ Reads: 1,300K+ ops/sec (1.21x RocksDB)
- ✅ Scans: 27,000+ scans/sec (1.27x RocksDB)
- ✅ Mixed: 450K+ ops/sec (1.11x RocksDB)

### Non-Negotiable
- ✅ Write amplification: Maintain 1.01x (no regression)
- ✅ All tests passing (120 tests)
- ✅ Data integrity: Zero data loss

---

## Next Steps (Immediate)

1. **Profile write path** (Day 1)
   - Install flamegraph: `cargo install flamegraph`
   - Run: `cargo flamegraph --example write_benchmark`
   - Identify top 3 bottlenecks

2. **Profile read path** (Day 1)
   - Run: `cargo flamegraph --example read_benchmark`
   - Identify bloom filter, cache, ALEX overhead

3. **Profile range scan path** (Day 1)
   - Run: `cargo flamegraph --example range_benchmark`
   - Identify merge, iterator, block loading overhead

4. **Prioritize optimizations** (Day 2)
   - Rank by expected impact × ease of implementation
   - Start with highest ROI optimizations

5. **Implement Phase 8** (Days 3-10)
   - WAL batching tuning
   - Blocked bloom filters
   - Adaptive readahead

**Timeline**: Start profiling NOW, Phase 8 complete in 2 weeks

---

**Updated**: November 7, 2025
**Status**: Plan created, ready to execute
**Priority**: 🔴 HIGH - Beat RocksDB is the goal

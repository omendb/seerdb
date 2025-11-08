# TODO - seerdb

**Last Updated**: November 7, 2025
**Current Focus**: Read Performance Optimization → Best-in-Class All Workloads
**Status**: 2/4 workloads best-in-class, read performance critical path

---

## Current Performance (Nov 7, 2025)

| Workload | seerdb | RocksDB | fjall | Best | vs Best | Status |
|----------|--------|---------|-------|------|---------|--------|
| **Writes** | **445K** | 363K | 430K | seerdb | **+3%** | ✅ **#1** |
| **Reads** | 403K | **1,048K** | 740K | RocksDB | **-62%** | ❌ #3 |
| **Mixed** | 252K | 403K | **581K** | fjall | **-57%** | ❌ #3 |
| **Scans** | **24K** | 20K | 12K | seerdb | **+97%** | ✅ **#1** |

**Write Amplification**: 1.01x (4.82x better than traditional LSM) 🏆 **BEST-IN-CLASS**

### Goal: Best-in-Class ALL Workloads

**Target Performance** (Conservative):
- ✅ Writes: 445K (maintain #1 position)
- 🎯 Reads: 800K+ (+99%, beat fjall 740K)
- 🎯 Mixed: 600K+ (+138%, beat fjall 581K)
- ✅ Scans: 24K (maintain #1 position)

**Stretch Goal** (Ambitious):
- 🏆 Reads: 1,000K+ (match RocksDB)
- 🏆 Mixed: 650K+ (beat all by 10%+)

---

## CRITICAL PATH: Read Performance Optimization

**Current bottleneck**: Reads are 2.6x slower than RocksDB, 1.8x slower than fjall

**Root cause identified** (via profiling on Nov 7):
1. **Block loading/decoding overhead** (PRIMARY) - 2.5x slower than cache potential
2. **Low cache hit rate** (LIKELY) - 749K potential vs 295K actual
3. ✅ Bloom filter (SOLVED) - Fixed in `b3a74df` (+7.7%)
4. ⚪ ALEX index (NOT THE FIX) - API mismatch, needs refactoring
5. **Mutex overhead** (POTENTIAL) - Two locks per read

**Estimated impact** of fixes:
- Cache hit rate optimization: +20-30% → ~500K reads/sec
- Block decoding optimization: +20-30% → ~650K reads/sec
- Mutex overhead reduction: +10-20% → ~750K reads/sec
- **Total potential**: 750-850K reads/sec (match/beat fjall)

---

## Phase 10: Read Performance Optimization (2-3 weeks) 🔴 CRITICAL

**Goal**: 403K → 800K+ reads (+99%) to beat fjall (740K)
**Priority**: HIGHEST - blocks "best-in-class all workloads" claim

### Priority 1: Block Cache Instrumentation & Optimization (2-3 days) ⭐⭐⭐

**Expected**: +20-30% reads (500K ops/sec)
**Complexity**: Low
**Status**: Not started

**Why this first**:
- Profiling shows 749K ops/sec potential (cache hits) vs 295K actual
- Suggests low hit rate or ineffective cache
- Quick win - instrumentation is easy, optimization is targeted

**Implementation** (src/sstable/mod.rs):
- [ ] Add cache hit/miss counters to SSTable struct
  - `cache_hits: AtomicU64`
  - `cache_misses: AtomicU64`
- [ ] Increment on cache hit/miss in get_data_block()
- [ ] Add DBStats fields for cache_hit_rate
- [ ] Expose via DB::stats()
- [ ] Add benchmark that prints cache hit rate
- [ ] Run baseline benchmark, measure hit rate

**Optimization paths** (after measurement):
- If hit rate < 30%: Increase block cache size (current: unbounded HashMap)
- If hit rate 30-60%: Implement LRU eviction (currently no eviction!)
- If hit rate > 60%: Block decoding is the bottleneck (move to Priority 2)

**Success Criteria**:
- Cache hit rate measured and visible
- If low hit rate: Increase cache size, verify improvement
- Expected: 403K → 500K reads (+24%)

---

### Priority 2: Flamegraph Profiling (1 day) ⭐⭐⭐

**Expected**: Identify exact hottest functions
**Complexity**: Low
**Status**: Not started

**Why this second**:
- Cache optimization may reveal next bottleneck
- Flamegraph shows CPU time distribution across ALL functions
- Can validate "block decoding" hypothesis

**Implementation**:
```bash
# Install flamegraph tools
cargo install flamegraph

# Profile baseline benchmark
cargo flamegraph --release --features baseline-benchmarks \
  --example baseline_benchmark -- --bench

# Analyze flamegraph.svg
# Look for hot paths in:
# - Block::decode()
# - Prefix decompression
# - Varint decoding
# - Mutex locking
```

**Success Criteria**:
- Flamegraph generated for read workload
- Top 3 hottest functions identified
- Validation of block decoding hypothesis

---

### Priority 3: Block Decoding Optimization (3-5 days) ⭐⭐⭐

**Expected**: +20-30% reads (650K ops/sec after cache fix)
**Complexity**: Medium
**Status**: Not started

**Why this third**:
- Profiling shows 2.5x gap between cache hits and SSTable reads
- Likely causes: prefix decompression, varint decoding, checksum
- Flamegraph will show exact bottleneck

**Sub-tasks** (prioritize after flamegraph):

#### 3a. Optimize Prefix Decompression
- [ ] Profile current implementation (src/sstable/block.rs)
- [ ] Check if shared_prefix reconstruction is hot
- [ ] Consider caching last reconstructed key in iterator
- [ ] Benchmark improvement

#### 3b. Optimize Varint Decoding
- [ ] Profile varint_decode() calls (key_len, value_len)
- [ ] Consider SIMD varint decoding (if hot)
- [ ] Or switch to fixed-length encoding for small values
- [ ] Benchmark improvement

#### 3c. Optimize Checksum Verification
- [ ] Profile checksum validation overhead
- [ ] Consider SIMD CRC32 (hardware acceleration)
- [ ] Or make checksums optional for reads (verify on write only)
- [ ] Benchmark improvement

#### 3d. Optimize Block Structure
- [ ] Consider zero-copy block access (avoid deserialization)
- [ ] Mmap blocks instead of read() + decode
- [ ] Benchmark improvement

**Success Criteria**:
- Block decoding 2x faster (measured in flamegraph)
- Read throughput: 500K → 650K ops/sec (+30%)
- All 141 tests passing

---

### Priority 4: Reduce Mutex Overhead (2-3 days) ⭐⭐

**Expected**: +10-20% reads (750K ops/sec after cache + decoding)
**Complexity**: Medium
**Status**: Not started

**Why this fourth**:
- Two locks per read: sstable_cache lock + SSTable lock
- RocksDB likely uses lockless reads
- Only optimize if flamegraph shows lock contention

**Implementation** (src/db.rs, src/sstable/mod.rs):

#### 4a. Measure Lock Contention
- [ ] Use `cargo flamegraph` to see time in lock acquisition
- [ ] Add instrumentation for lock wait time
- [ ] Decide if optimization is worth it

#### 4b. Optimize SSTable Cache Lock
- [ ] Replace `HashMap<u64, Arc<Mutex<SSTable>>>` with `DashMap`
- [ ] DashMap is lock-free concurrent hashmap
- [ ] Eliminates sstable_cache lock
- [ ] Benchmark improvement

#### 4c. Optimize SSTable Lock
- [ ] Change `Mutex<File>` to `RwLock<File>` (if needed)
- [ ] Allow concurrent readers
- [ ] Or use lockless reads with atomic file handle
- [ ] Benchmark improvement

**Success Criteria**:
- Lock contention < 5% of CPU time (flamegraph)
- Read throughput: 650K → 750K ops/sec (+15%)
- All 141 tests passing

---

### Priority 5: ALEX Efficient Lower Bound (5-7 days) ⭐

**Expected**: +10-15% reads (850K ops/sec)
**Complexity**: High
**Status**: Not started (lower_bound() added but slow)

**Why this last**:
- ALEX is O(1) vs partition_point O(log n), but log(100) = 7 iterations
- Only 7x improvement on one step of many
- Requires significant GappedNode refactoring
- Lower priority than cache, decoding, locks

**Implementation** (src/alex/gapped_node.rs):

- [ ] Add `lower_bound_key(search_key: i64) -> Option<i64>` to GappedNode
  - Use linear model to predict position
  - Scan forward from predicted position (within error bound)
  - Return first key >= search_key WITHOUT cloning value
  - O(1) amortized (error bound is constant)

- [ ] Update AlexTree::lower_bound() to use lower_bound_key()
  - First call: lower_bound_key(search_key) → get key only
  - Second call: get(key) → fetch value for that key
  - Two lookups but both O(1), vs current materialization

- [ ] Re-enable ALEX in src/sstable/mod.rs:564
  - Remove `if false &&`
  - Use new efficient lower_bound()

- [ ] Benchmark improvement
  - Measure index lookup time before/after
  - Measure overall read throughput

**Success Criteria**:
- ALEX lower_bound() faster than partition_point (benchmark)
- Read throughput: 750K → 850K ops/sec (+13%)
- All 141 tests passing

---

## Phase 11: Mixed Workload Optimization (1 week) 🟡 HIGH

**Goal**: 252K → 600K+ mixed (+138%) to beat fjall (581K)
**Priority**: HIGH - depends on read optimizations

**Current hypothesis**: Mixed workload is slow because reads are slow
- Mixed = 50% reads + 50% writes
- Writes: 445K (fast) ✅
- Reads: 403K (slow) ❌
- Expected mixed: ~(445K + 403K) / 2 = 424K theoretical
- Actual mixed: 252K (59% of theoretical)

**Gap analysis**: 424K theoretical vs 252K actual = 172K missing (40% overhead)

**Likely causes**:
1. **Write stalls** - Reads slow down flushes, which block writes
2. **Lock contention** - Writes and reads compete for same locks
3. **Cache pollution** - Writes evict read cache entries

### Sub-task 1: Investigate Write Stalls in Mixed Workload

- [ ] Add instrumentation to measure flush time during mixed workload
- [ ] Measure write blocking time (waiting for flush)
- [ ] If significant: Optimize flush to not block writes
  - Consider double-buffering memtables
  - Or async flush with queue

### Sub-task 2: Reduce Lock Contention

- [ ] Measure lock contention in mixed workload (flamegraph)
- [ ] If partitioned memtables help: Verify 16 partitions is optimal
- [ ] If SSTable locks are hot: Use RwLock for concurrent read/write

### Sub-task 3: Optimize Cache for Mixed Workload

- [ ] Implement LRU eviction (not random)
- [ ] Consider separate cache for reads vs writes (if applicable)
- [ ] Benchmark cache hit rate in mixed workload

**Success Criteria**:
- After read optimizations: Mixed should be ~80% of theoretical max
- Mixed throughput: 252K → 600K+ ops/sec (beat fjall 581K)
- All 141 tests passing

---

## Timeline (3-4 weeks to best-in-class)

### Week 1: Cache + Flamegraph + Decoding (Priority 1-3)
- **Days 1-2**: Block cache instrumentation + measurement
- **Days 3**: Cache optimization (LRU, size tuning)
- **Day 4**: Flamegraph profiling
- **Days 5-7**: Block decoding optimization (based on flamegraph)
- **Expected**: 403K → 650K reads (+61%)

### Week 2: Mutex + ALEX (Priority 4-5)
- **Days 8-10**: Reduce mutex overhead (DashMap, RwLock)
- **Days 11-14**: ALEX efficient lower_bound (GappedNode refactor)
- **Expected**: 650K → 850K reads (+31%)

### Week 3: Mixed Workload Optimization
- **Days 15-17**: Investigate write stalls and lock contention
- **Days 18-19**: Cache optimization for mixed workload
- **Days 20-21**: Benchmark and validate
- **Expected**: 252K → 600K+ mixed (+138%)

### Week 4: Polish + Validation
- **Days 22-24**: Final optimizations based on profiling
- **Days 25-26**: Comprehensive benchmarking (all workloads)
- **Days 27-28**: Documentation and commit
- **Expected**: All workloads best-in-class ✅

**Total timeline**: 3-4 weeks to best-in-class performance

---

## Success Metrics

### Phase 10 Complete (Read Optimization)
- ✅ Reads: 800K+ ops/sec (beat fjall 740K)
- ✅ Cache hit rate: 60%+ measured and optimized
- ✅ Flamegraph shows no single hot function >20% CPU
- ✅ Block decoding 2x faster than baseline
- ✅ Lock overhead < 5% CPU time
- ✅ All 141 tests passing

### Phase 11 Complete (Mixed Optimization)
- ✅ Mixed: 600K+ ops/sec (beat fjall 581K)
- ✅ Mixed = ~80% of (read_perf + write_perf) / 2 theoretical max
- ✅ All 141 tests passing

### Best-in-Class Achievement 🏆
- ✅ Writes: #1 (445K vs RocksDB 363K, fjall 430K)
- ✅ Reads: #1 or #2 (800K+ vs fjall 740K, RocksDB 1,048K)
- ✅ Mixed: #1 or #2 (600K+ vs fjall 581K, RocksDB 403K)
- ✅ Scans: #1 (24K vs RocksDB 20K, fjall 12K)
- ✅ Write amp: #1 (1.01x vs 4.88x traditional)
- ✅ Production ready for all workload types

**Marketing claim**: "Best-in-class performance across ALL workloads"

---

## Completed Optimizations (Still Active)

### ✅ Phase 9.4: Dostoevsky Adaptive Compaction
- Workload-aware LSM tuning with dynamic size ratio
- All 141 tests passing

### ✅ Phase 9.3: Partitioned Memtables
- 16 hash-partitioned memtables using xxhash
- 2.14x multi-threaded speedup (466K ops/sec with 8 threads)
- Reduced lock contention 16x

### ✅ Phase 9.2: Portable SIMD Foundation
- Cross-platform SIMD for key operations
- Prefix compression uses SIMD

### ✅ Phase 9.1: Prefix Compression
- 31% space savings with zero throughput regression
- Block-level compression with restart points

### ✅ Bloom Filter Optimization (Nov 7, 2025)
- Removed redundant bloom filter check
- +7.7% read improvement
- Commit: `b3a74df`

### ✅ Comprehensive Profiling Suite (Nov 7, 2025)
- `examples/read_profiling_detailed.rs` - 5 read patterns
- `examples/bloom_filter_analysis.rs` - False positive testing
- `examples/sstable_count_check.rs` - Structure verification
- Identified block decoding as primary bottleneck

---

## Not Implementing (Rejected Optimizations)

### ❌ ALEX with Current API
- Investigated Nov 7, 2025
- 45% performance regression due to pairs() materialization
- Requires efficient lower_bound API (Priority 5)
- See: /tmp/alex_investigation_nov7.md

### ❌ Parameter Tweaking Without Research
- Changing memtable size without justification
- Adjusting batch sizes arbitrarily
- Tuning level ratios without Dostoevsky math

---

## Research Papers Backing This Plan

**Cache Optimization**:
- "LRU-K: An Efficient Cache Replacement Algorithm" (O'Neil et al., 1993)
- "ARC: A Self-Tuning, Low Overhead Replacement Cache" (Megiddo et al., 2003)

**Block Decoding**:
- "Prefix Compression" - Standard in LevelDB, RocksDB
- "Varint Encoding" - Protocol Buffers, standard practice
- "SIMD-accelerated CRC32" - Intel optimization guides

**Lock-Free Data Structures**:
- "DashMap: Concurrent HashMap" - Rust concurrent-hashmap crate
- "RwLock Performance" - Standard Rust concurrency primitive

**ALEX Learned Index**:
- "ALEX: An Updatable Adaptive Learned Index" (Ding et al., MIT 2020)
- Already implemented, needs efficient API

---

## Immediate Next Steps (This Session)

### 1. Block Cache Instrumentation (Day 1)
- [ ] Add cache hit/miss counters to SSTable
- [ ] Add DBStats fields for cache metrics
- [ ] Run baseline benchmark with cache stats
- [ ] Document current cache hit rate

**Expected outcome**: Know if cache is the problem (hit rate < 60%)

### 2. If Low Hit Rate: Cache Optimization (Day 2-3)
- [ ] Implement LRU eviction policy
- [ ] Tune cache size (currently unbounded)
- [ ] Re-benchmark with cache stats
- [ ] Measure improvement

**Expected outcome**: 403K → 500K reads (+24%)

### 3. Flamegraph Profiling (Day 4)
- [ ] Install flamegraph tools
- [ ] Profile read workload
- [ ] Identify top 3 hottest functions
- [ ] Validate block decoding hypothesis

**Expected outcome**: Clear path to block decoding optimization

---

**Current Status**: Phase 10 ready to start
**Next Action**: Implement block cache instrumentation
**Timeline**: 3-4 weeks to best-in-class all workloads
**Priority**: 🔴 CRITICAL - Read performance is the gap

**References**:
- `/tmp/session_summary_nov7_final.md` - Latest profiling findings
- `/tmp/alex_investigation_nov7.md` - ALEX investigation
- `ai/STATUS.md` - Current performance baseline

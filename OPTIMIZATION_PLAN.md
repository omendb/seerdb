# seerdb Optimization Plan - Beat RocksDB

**Created**: November 7, 2025
**Status**: Phase 7 Complete - Ready for Phase 8
**Goal**: Match or exceed RocksDB performance while maintaining best-in-class write amplification

---

## Executive Summary

**Current State**: Production-ready with competitive performance
- ✅ Range scans: 19.6x improvement (SSTable filtering)
- ✅ Write amplification: 4.82x better than RocksDB (1.01x vs 4.88x)
- ⚠️ Performance gaps: 19-39% slower than RocksDB across workloads
- 🎯 Goal: Match or beat RocksDB in all workloads (4-7 weeks)

**Unique Strengths**:
- Only Rust LSM with learned components (ALEX indexes + learned bloom filters)
- Best-in-class write amplification (4.82x better than traditional LSM)
- Safe Rust (no unsafe code in core engine)

---

## Current Performance (After Phase 7)

**Benchmark**: baseline_benchmark (100K ops, 1KB values)

| Workload | seerdb | RocksDB | Ratio | Gap | Status |
|----------|--------|---------|-------|-----|--------|
| **Writes** | 218K ops/sec | 356K | **0.61x** | -39% | 🔴 **BIGGEST GAP** |
| **Reads** | 872K ops/sec | 1,070K | **0.81x** | -19% | ⚠️ Competitive |
| **Mixed** | 311K ops/sec | 407K | **0.76x** | -24% | ⚠️ Acceptable |
| **Scans** | 17,087/sec | 21,175 | **0.81x** | -19% | ⚠️ Competitive |
| **Write Amp** | **1.01x** | 4.88x | **4.82x better** | - | ✅ **BEST** |

**vs Other Rust Engines**:
- fjall: 50% faster scans, similar writes, 4.82x better write amp
- sled: 3x faster writes (LSM vs B-tree), slower reads

---

## Phase 7 Achievement: SSTable Range Filtering

**Problem**: Range scans were 95% slower than RocksDB (870 vs 17,332 scans/sec)

**Root Cause**: Creating iterators for ALL SSTables, even non-overlapping ones

**Solution**: Add min_key/max_key metadata, filter SSTables by range overlap

**Implementation** (commit 5e4dc0c):
```rust
// SSTable metadata
pub struct SSTable {
    min_key: Option<Bytes>,  // First key in SSTable
    max_key: Option<Bytes>,  // Last key in SSTable
}

// Filter before creating iterators
if sstable.overlaps_range(start_key, end_key) {
    iterators.push(sstable.scan_range(start_key, end_key));
}
```

**Results**:
- **870 → 17,087 scans/sec** (19.6x improvement!)
- **0.04x → 0.81x RocksDB** (competitive!)
- **50% faster than fjall**

**How it works**:
- Query: [key_100, key_200)
- SSTable A [key_000, key_050): **SKIP**
- SSTable B [key_100, key_150): **INCLUDE**
- SSTable C [key_250, key_300): **SKIP**
- Result: Create 1 iterator instead of 3

---

## Optimization Strategy (3 Phases)

### Phase 8: Close the Gap (1-2 weeks) → 0.9x RocksDB

**Goal**: Reach 90%+ of RocksDB performance

**Priority 1: Write Path** (-39% gap, 138K ops/sec)

Known bottlenecks:
- WAL write time: 48.5% of total time
- Memtable insert: ~30% of time
- Lock contention: TBD (needs profiling)

Optimizations:
1. **Tokio Async I/O** (3-4 days)
   - Replace std::fs with tokio::fs
   - Async WAL writes (non-blocking)
   - Batch multiple writes efficiently
   - **Expected**: +30-50% write throughput
   - **Why Tokio**: Safe (pure Rust), cross-platform (macOS + Linux), battle-tested

2. **WAL Batching Tuning** (1 day)
   - Increase batch size: 8MB → 16MB
   - Reduce flush interval: 100ms → 50ms
   - **Expected**: +10-15% if not using async I/O

3. **Memtable Optimization** (1 day)
   - Profile lock contention (crossbeam-skiplist)
   - Pre-allocate capacity
   - Reduce allocation overhead
   - **Expected**: +5-10%

4. **Allocation Pooling** (1 day)
   - Pool BytesMut for WAL records
   - Reuse buffers across writes
   - **Expected**: +5-10%

**Target**: 270K+ ops/sec (0.76x RocksDB)

**Priority 2: Read Path** (-19% gap, 198K ops/sec)

Potential bottlenecks:
- Bloom filter: May be slower than RocksDB's
- Block cache: LRU may not be optimal
- ALEX index: May have overhead vs binary search

Optimizations:
1. **Blocked Bloom Filters** (2 days)
   - Implement cache-friendly blocked bloom (64-byte blocks)
   - Research shows 3x faster than traditional
   - Align to CPU cache lines
   - **Expected**: +10-15% read throughput

2. **Block Cache Tuning** (1 day)
   - Increase default: 8MB → 32MB
   - Consider LFU policy instead of LRU
   - **Expected**: +5-10% via better hit rate

3. **ALEX Index Profiling** (1 day)
   - Measure ALEX vs binary search overhead
   - Tune model complexity for 1M key SSTables
   - **Expected**: +5-10%

**Target**: 1,020K+ ops/sec (0.95x RocksDB)

**Priority 3: Range Scans** (-19% gap, 4,088 scans/sec)

Optimizations:
1. **Adaptive Readahead** (1-2 days)
   - Detect sequential access pattern
   - Prefetch next 2-4 blocks in background
   - RocksDB uses this (proven approach)
   - **Expected**: +25-40% scan throughput

2. **Block Cache Warming** (0.5 day)
   - Keep recently scanned blocks in cache
   - Increase priority for scan blocks
   - **Expected**: +5-10%

**Target**: 20,000+ scans/sec (0.94x RocksDB)

---

### Phase 9: Match RocksDB (1-2 weeks) → 1.0x RocksDB

**Goal**: Match or slightly exceed RocksDB

**1. SIMD Optimizations** (2-3 days)

**SIMD Key Comparisons** (1 day):
- Vectorize memcmp in k-way merge
- Use AVX2 on x86, NEON on ARM
- **Expected**: +20-30% merge speed → +15-20% scan throughput

**SIMD Bloom Filters** (1 day):
- Parallel hash computation
- Vectorized bit checks
- **Expected**: +30-50% bloom speed → +10-15% read throughput

**Platform support**:
```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

// Fallback for non-SIMD
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn fallback_implementation() { ... }
```

**2. Lock-Free Memtable** (3-4 days)

Current: crossbeam-skiplist (uses locks)
Target: Custom lock-free implementation with atomic operations

**Expected**: +15-25% write throughput

**Risk**: High complexity, subtle bugs possible

**3. Advanced WAL Optimizations** (2 days)

- Group commit: Batch writes from multiple threads
- Double buffering: Write to buffer while flushing previous
- **Expected**: +10-20% write throughput

**Phase 9 Targets**:
- Writes: 360K+ ops/sec (1.01x RocksDB) ✅
- Reads: 1,070K+ ops/sec (1.0x RocksDB) ✅
- Scans: 24,000+ scans/sec (1.13x RocksDB) ✅
- Mixed: 410K+ ops/sec (1.01x RocksDB) ✅

---

### Phase 10: Beat RocksDB (2-3 weeks) → 1.2x+ RocksDB

**Goal**: Exceed RocksDB through workload-aware optimization

**1. Workload-Aware Auto-Tuning** (1 week)

Based on CAMAL paper (Sept 2024):

**Detect workload characteristics**:
- Key sortedness: Measure % of keys in order
- Read/write ratio: Track operations
- Value size distribution: Average, median, p99
- Hot/cold key distribution: Access frequency

**Auto-tune parameters**:
- Sorted keys (>90%) → Tiered compaction
- Random keys (<50%) → Leveled compaction
- Large values (>4KB) → Aggressive vLog threshold (512B)
- Read-heavy (>70% reads) → Larger bloom filters (10 bits/key)
- Write-heavy (>70% writes) → Smaller blooms (5 bits/key)

**Implementation**:
```rust
pub struct WorkloadDetector {
    read_count: AtomicU64,
    write_count: AtomicU64,
    sorted_ratio: f64,  // Updated on memtable flush
    avg_value_size: usize,
}

impl DB {
    fn auto_tune(&mut self) {
        let metrics = self.detector.snapshot();

        if metrics.sorted_ratio > 0.9 {
            self.set_compaction(Tiered);  // Better for sorted
        }

        if metrics.read_ratio > 0.7 {
            self.set_bloom_bits(10);  // Larger blooms
        }
    }
}
```

**Expected**: +20-30% on optimized workloads

**2. Advanced Bloom Filters** (1 week)

**Learned Bloom Filters**:
- Neural network or decision tree model
- 90% space reduction (from research)
- Train on SSTable keys during compaction

**Trade-off**:
- ✅ 90% space reduction
- ✅ Better cache utilization
- ❌ Model inference latency
- ❌ Training cost

**Expected**: +10-20% reads (from better cache utilization)

**3. Read Hotness Tracking** (1 week)

Based on "LSM-Tree + Read Hotness + Learned Index" (Oct 2025):

**Track access frequency**:
- Bloom filter in block cache (count-min sketch)
- Identify hot keys (>100 accesses)

**Optimize for hot keys**:
- ALEX: More complex model for hot keys
- Block cache: Higher priority for hot key blocks
- Prefetching: Aggressive for hot key ranges

**Expected**: +15-25% on read-heavy workloads

**Phase 10 Targets**:
- Writes: 400K+ ops/sec (1.12x RocksDB) 🎯
- Reads: 1,300K+ ops/sec (1.21x RocksDB) 🎯
- Scans: 27,000+ scans/sec (1.27x RocksDB) 🎯
- Mixed: 450K+ ops/sec (1.11x RocksDB) 🎯

---

## Why Tokio Instead of io_uring

**Decision**: Use tokio::fs for async I/O (not io_uring)

**Rationale**:
1. **Security**: io_uring has had several CVEs (security concerns)
2. **Cross-platform**: Tokio works on macOS (development) + Linux (production)
3. **Safety**: Pure Rust, no unsafe kernel interface
4. **Battle-tested**: Used by thousands of production Rust applications
5. **Performance**: Close enough to io_uring for our needs

**io_uring issues**:
- CVE-2023-2598, CVE-2023-0266, CVE-2022-29582 (kernel vulnerabilities)
- Linux-only (can't develop on macOS)
- Requires kernel 5.1+ (compatibility issues)
- Complex unsafe interface (harder to get right)

**Tokio benefits**:
- Async/await: Native Rust async
- epoll (Linux), kqueue (macOS): Mature, secure APIs
- Non-blocking I/O: Similar benefits to io_uring
- Ecosystem: Integrates with tokio-based tools

**Expected performance**:
- io_uring: +50-100% I/O throughput (Linux-only)
- Tokio: +30-50% I/O throughput (cross-platform, safer)
- Trade-off: 20-30% less improvement, but safer and portable

**Conclusion**: Tokio is the right choice for seerdb

---

## Implementation Roadmap

### Week 11 (Nov 7-13): Phase 8 Part 1

**Day 1-2**: Profiling
- Install flamegraph: `cargo install flamegraph`
- Profile write path: `cargo flamegraph --example write_benchmark`
- Profile read path: `cargo flamegraph --example read_benchmark`
- Profile scan path: `cargo flamegraph --example range_benchmark`
- Identify top 3 bottlenecks per path

**Day 3-5**: Tokio Async I/O
- Add tokio dependency: `tokio = { version = "1.0", features = ["full"] }`
- Replace std::fs with tokio::fs in WAL
- Make DB::put() async (or spawn write tasks)
- Benchmark: Expect +30-50% write throughput

**Day 6-7**: Blocked Bloom Filters
- Implement 64-byte cache-aligned blocks
- Replace traditional bloom filter
- Benchmark: Expect +10-15% read throughput

### Week 12 (Nov 14-20): Phase 8 Part 2

**Day 1-2**: Adaptive Readahead
- Detect sequential access (consecutive block reads)
- Prefetch next 2-4 blocks
- Benchmark: Expect +25-40% scan throughput

**Day 3-4**: Memtable Optimization
- Profile lock contention
- Optimize allocation patterns
- Benchmark: Expect +5-10% write throughput

**Day 5-7**: Benchmarking and Validation
- Run baseline_benchmark with all optimizations
- Compare vs RocksDB, fjall, sled
- Target: 0.9x+ RocksDB across all workloads

### Week 13-14 (Nov 21-Dec 4): Phase 9

**SIMD Optimizations** (3 days)
**Lock-Free Memtable** (4 days)
**Advanced WAL** (2 days)
**Benchmarking** (2 days)

**Target**: 1.0x+ RocksDB

### Week 15-17 (Dec 5-25): Phase 10

**Workload-Aware Tuning** (1 week)
**Advanced Bloom Filters** (1 week)
**Read Hotness Tracking** (1 week)

**Target**: 1.2x+ RocksDB

---

## Technical Details

### Tokio Integration Plan

**Current (blocking)**:
```rust
impl WAL {
    pub fn append(&mut self, record: &[u8]) -> Result<()> {
        self.file.write_all(record)?;  // Blocks thread
        Ok(())
    }
}
```

**After (async)**:
```rust
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

impl WAL {
    pub async fn append(&mut self, record: &[u8]) -> Result<()> {
        self.file.write_all(record).await?;  // Non-blocking
        Ok(())
    }
}

// Or use spawn for background writes
impl DB {
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let record = self.encode_record(key, value);

        // Spawn async write task
        let wal = self.wal.clone();
        tokio::spawn(async move {
            wal.append(&record).await
        });

        // Memtable insert (sync)
        self.memtable.put(key, value);
        Ok(())
    }
}
```

**Benefits**:
- Non-blocking: Thread continues while I/O in progress
- Batching: Tokio can batch multiple writes
- Scalability: Handle more concurrent writes

### Blocked Bloom Filter Design

**Traditional Bloom**:
- Single bit array
- k hash functions scattered across array
- Poor cache locality (k cache misses per query)

**Blocked Bloom**:
- Divide into 64-byte blocks (cache line size)
- All k hash functions within one block
- 1 cache miss per query (vs k misses)

**Implementation**:
```rust
pub struct BlockedBloomFilter {
    blocks: Vec<[u8; 64]>,  // Array of 64-byte blocks
    num_blocks: usize,
    k: usize,  // Hash functions per block
}

impl BlockedBloomFilter {
    pub fn contains(&self, key: &[u8]) -> bool {
        let block_idx = hash(key) % self.num_blocks;
        let block = &self.blocks[block_idx];  // 1 cache miss

        // All k checks within this block (0 additional misses)
        for i in 0..self.k {
            let bit_idx = hash_k(key, i) % 512;  // 512 bits per block
            if !block.get_bit(bit_idx) {
                return false;
            }
        }
        true
    }
}
```

**Performance**:
- Traditional: k cache misses per query (k=7 typical)
- Blocked: 1 cache miss per query
- Speedup: 3-7x (research shows 3x average)

---

## Risk Assessment

### Low Risk (High confidence)
- ✅ Tokio async I/O: Battle-tested, safe, well-documented
- ✅ Blocked bloom filters: Well-researched, clear implementation
- ✅ Adaptive readahead: Standard optimization, used by RocksDB

### Medium Risk (Moderate confidence)
- ⚠️ SIMD: Platform-specific, needs fallback for non-SIMD CPUs
- ⚠️ Memtable optimization: May not find significant improvements
- ⚠️ Workload detection: Heuristics may not generalize well

### High Risk (Lower confidence)
- 🔴 Lock-free memtable: Complex, subtle bugs possible
- 🔴 Learned bloom filters: Training overhead may hurt performance
- 🔴 Advanced caching: Complex policies may not always help

**Mitigation**:
- Start with low-risk optimizations (Phase 8)
- Validate each optimization with benchmarks
- Keep rollback path for failed optimizations
- Don't sacrifice write amplification (our key advantage)

---

## Success Metrics

### Phase 8 Success (2 weeks)
- ✅ Writes: 270K+ ops/sec (0.76x RocksDB)
- ✅ Reads: 1,020K+ ops/sec (0.95x RocksDB)
- ✅ Scans: 20,000+ scans/sec (0.94x RocksDB)
- ✅ Mixed: 360K+ ops/sec (0.88x RocksDB)

### Phase 9 Success (4 weeks total)
- ✅ Writes: 360K+ ops/sec (1.01x RocksDB)
- ✅ Reads: 1,070K+ ops/sec (1.0x RocksDB)
- ✅ Scans: 24,000+ scans/sec (1.13x RocksDB)
- ✅ Mixed: 410K+ ops/sec (1.01x RocksDB)

### Phase 10 Success (7 weeks total)
- ✅ Writes: 400K+ ops/sec (1.12x RocksDB)
- ✅ Reads: 1,300K+ ops/sec (1.21x RocksDB)
- ✅ Scans: 27,000+ scans/sec (1.27x RocksDB)
- ✅ Mixed: 450K+ ops/sec (1.11x RocksDB)

### Non-Negotiable
- ✅ Write amplification: Maintain 1.01x (4.82x better than RocksDB)
- ✅ All tests passing (120 tests)
- ✅ Data integrity: Zero data loss under failures
- ✅ Safe Rust: No unsafe code in core paths

---

## Next Steps (Immediate)

**Today** (Nov 7):
1. Install profiling tools
   ```bash
   cargo install flamegraph
   cargo install cargo-instruments  # macOS specific
   ```

2. Profile all workloads
   ```bash
   cargo flamegraph --release --example write_benchmark
   cargo flamegraph --release --example read_benchmark
   cargo flamegraph --release --example range_benchmark
   ```

3. Analyze results
   - Identify top 3 bottlenecks per workload
   - Validate 48.5% WAL time assumption
   - Find unexpected hotspots

**Tomorrow** (Nov 8):
1. Start Tokio integration
   - Add tokio dependency
   - Create async WAL prototype
   - Benchmark isolated WAL performance

2. Document findings
   - Update OPTIMIZATION_PLAN.md with profiling results
   - Create detailed implementation plan for top optimizations

**This Week** (Nov 7-13):
- Complete Tokio async I/O integration
- Implement blocked bloom filters
- Benchmark improvements
- Target: +30-50% writes, +10-15% reads

---

## References

**Research Papers**:
- "CAMAL: Optimizing LSM-trees via Active Learning" (Sept 2024)
- "Evaluating Learned Indexes in LSM-tree Systems" (June 2025)
- "Bf-Tree: Modern Read-Write-Optimized Range Index" (VLDB 2024)
- "LSM-Tree + Read Hotness + Learned Index" (Oct 2025)

**Implementation References**:
- RocksDB source: SSTable filtering, adaptive readahead
- fjall source: Blocked bloom filters, cache strategies
- Tokio docs: Async I/O patterns, best practices

**Internal Docs**:
- `ai/research/COMPETITIVE_ANALYSIS.md`: Engine comparisons
- `ai/research/SOTA_RESEARCH_2024_2025.md`: Latest research
- `ai/design/BEAT_ROCKSDB_PLAN.md`: Detailed optimization plan
- `ai/STATUS.md`: Current status and results

---

**Last Updated**: November 7, 2025
**Status**: Ready to execute Phase 8
**Priority**: 🔴 HIGH - Beat RocksDB is the primary goal

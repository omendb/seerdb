# TODO - seerdb

**Last Updated**: November 7, 2025
**Current Focus**: Beat RocksDB in all workloads

---

## ✅ Phase 7 Complete: SSTable Range Filtering (Nov 7, 2025)

**Achievement**: 19.6x range scan improvement - now competitive with RocksDB!

### Results
- Range scans: **870 → 17,087 scans/sec** (19.6x improvement!)
- Ratio vs RocksDB: **0.04x → 0.81x** (competitive!)
- Ratio vs fjall: **0.08x → 1.50x** (50% faster!)

### Implementation (commit 5e4dc0c)
- [x] Added min_key/max_key metadata to SSTable (v1 format)
- [x] Track first/last keys during build
- [x] Implemented overlaps_range() method
- [x] Filter SSTables in db.range() before creating iterators
- [x] Benchmarked: 17,087 scans/sec (0.81x RocksDB)
- [x] Updated all documentation

**Status**: Production-ready for all workloads ✅

---

## 🎯 NEW FOCUS: Beat RocksDB

**Goal**: Match or exceed RocksDB performance in all workloads

### Current Performance (baseline_benchmark)

| Workload | seerdb | RocksDB | Ratio | Gap |
|----------|--------|---------|-------|-----|
| **Writes** | 218K | 356K | 0.61x | -39% (138K ops/sec) |
| **Reads** | 872K | 1,070K | 0.81x | -19% (198K ops/sec) |
| **Mixed** | 311K | 407K | 0.76x | -24% (96K ops/sec) |
| **Scans** | 17,087 | 21,175 | 0.81x | -19% (4,088 scans/sec) |

**Best-in-class**: Write amplification (1.01x vs 4.88x, 4.82x better) 🏆

### Priority Targets

1. **Writes: 0.61x → 1.0x+** (close 138K ops/sec gap)
2. **Reads: 0.81x → 1.0x+** (close 198K ops/sec gap)
3. **Scans: 0.81x → 1.0x+** (close 4,088 scans/sec gap)
4. **Mixed: 0.76x → 1.0x+** (close 96K ops/sec gap)

---

## Phase 8: Profile and Optimize (Week 11)

### Write Path Optimization (Priority 1)

**Gap**: 39% slower than RocksDB (218K vs 356K ops/sec)

**Tasks**:
- [ ] Profile write path end-to-end (flamegraph)
  - WAL write time (known bottleneck: 48.5% of time)
  - Memtable insert time
  - Lock contention
  - Allocation overhead
- [ ] Identify top 3 bottlenecks
- [ ] Implement optimizations:
  - [ ] WAL: Batch writes more aggressively (reduce syscalls)
  - [ ] WAL: Consider io_uring (zero-copy, zero-syscall)
  - [ ] Memtable: Lock-free skiplist (crossbeam-skiplist already used, but check contention)
  - [ ] Memtable: Pre-allocate capacity (reduce allocations)
- [ ] Benchmark each optimization
- [ ] Target: 300K+ ops/sec (0.84x+ RocksDB)

**Expected Impact**: +30-50% write throughput

### Read Path Optimization (Priority 2)

**Gap**: 19% slower than RocksDB (872K vs 1,070K ops/sec)

**Tasks**:
- [ ] Profile read path (flamegraph)
  - Bloom filter check time
  - ALEX index lookup time
  - Block cache hit/miss ratio
  - Block decompression time
- [ ] Identify bottlenecks
- [ ] Implement optimizations:
  - [ ] Bloom filter: Use blocked bloom (3x faster, from research)
  - [ ] ALEX: Measure overhead vs binary search
  - [ ] Block cache: Increase default size or use LFU instead of LRU
  - [ ] Prefetch: Implement readahead for sequential access
- [ ] Benchmark each optimization
- [ ] Target: 1,100K+ ops/sec (1.03x+ RocksDB)

**Expected Impact**: +25-35% read throughput

### Range Scan Optimization (Priority 3)

**Gap**: 19% slower than RocksDB (17,087 vs 21,175 scans/sec)

**Tasks**:
- [ ] Profile range scan path
  - Iterator creation time
  - K-way merge overhead
  - Block loading time
- [ ] Implement optimizations:
  - [ ] Adaptive readahead (prefetch next blocks)
  - [ ] SIMD key comparisons in k-way merge
  - [ ] Reduce iterator allocation overhead
- [ ] Benchmark
- [ ] Target: 22,000+ scans/sec (1.04x+ RocksDB)

**Expected Impact**: +25-30% scan throughput

---

## Research Validation (Optional - Confidence Building)

### Learned Component Impact Measurement

**Goal**: Quantify the benefit of learned components

1. **Learned Bloom Filter Space Savings** (1 day)
   - [ ] Implement traditional bloom filter baseline
   - [ ] Measure space: learned vs traditional
   - [ ] Measure FP rate (should be equivalent)
   - [ ] Measure query time overhead
   - [ ] Target: Validate 90% space reduction claim

2. **ALEX Index vs Binary Search** (1 day)
   - [ ] Toggle ALEX on/off in SSTable
   - [ ] Benchmark reads with/without ALEX
   - [ ] Measure memory overhead
   - [ ] Target: Quantify 20-40% read improvement (from SOTA papers)

3. **Write Amplification Validation** (1 day)
   - [ ] Measure disk writes over 1B operations
   - [ ] Compare: seerdb vs fjall (traditional LSM)
   - [ ] Calculate: bytes written / bytes ingested
   - [ ] Target: Confirm 4.82x improvement

---

## Advanced Optimizations (Phase 9-10)

### io_uring Integration (Phase 9, 4 days)

**Potential**: 2x faster I/O (from research)

- [ ] Replace std::fs with tokio-uring
- [ ] Batch SSTable reads during compaction
- [ ] Batch WAL writes
- [ ] Benchmark: Compaction speed, write throughput
- [ ] Expected: +50-100% compaction speed, +20-40% write throughput

### Workload-Aware Tuning (Phase 10, 7 days)

**Potential**: 20-30% throughput improvement (CAMAL paper)

- [ ] Implement workload detection
  - Key sortedness (sorted vs random)
  - Read/write ratio
  - Value size distribution
  - Hot/cold key distribution
- [ ] Adaptive parameter tuning
  - Compaction strategy (leveled vs tiered)
  - Bloom filter size
  - vLog threshold
  - Memtable size
- [ ] Benchmark on different workloads
- [ ] Expected: 20-30% improvement per workload type

---

## Timeline and Priorities

### Week 11 (Nov 7-13): Beat RocksDB
- **Focus**: Profile and optimize write/read/scan paths
- **Goal**: Match or exceed RocksDB in all workloads
- **Target**: 1.0x+ RocksDB across the board

### Week 12 (Nov 14-20): Validate Research Claims
- **Focus**: Measure learned component impact
- **Goal**: Quantify benefits of ALEX + learned blooms + vLog
- **Deliverable**: Research validation results

### Week 13-14 (Nov 21-Dec 4): Advanced Optimizations
- **Focus**: io_uring, workload-aware tuning
- **Goal**: Push beyond RocksDB (1.2x+ in key workloads)

---

## Success Metrics

### Phase 8 Success (Beat RocksDB)
- ✅ Writes: 1.0x+ RocksDB (356K+ ops/sec)
- ✅ Reads: 1.0x+ RocksDB (1,070K+ ops/sec)
- ✅ Mixed: 1.0x+ RocksDB (407K+ ops/sec)
- ✅ Scans: 1.0x+ RocksDB (21,175+ scans/sec)
- ✅ Write Amp: Maintain 1.01x (no regression)

### Phase 9-10 Success (Beyond RocksDB)
- ✅ Writes: 1.2x+ RocksDB with io_uring
- ✅ Reads: 1.2x+ RocksDB with optimized blooms
- ✅ Workload-specific: 1.3x+ on optimized workloads

---

**Next Action**: Profile write path to identify bottlenecks
**Timeline**: 1 day profiling + 3-5 days optimization = 4-6 days total
**Priority**: 🔴 HIGH - Close performance gap with RocksDB

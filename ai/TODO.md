# TODO - seerdb

**Last Updated**: November 8, 2025
**Current Sprint**: Close fjall Gap (Nov 8-15, 2025)
**Goal**: Beat fjall on mixed workload (+18% from 718K → 850K+)

---

## Current Performance (Nov 8, 2025 - After jemalloc)

| Workload | seerdb | RocksDB | fjall | vs RocksDB | vs fjall | Status |
|----------|--------|---------|-------|------------|----------|--------|
| **Writes** | **878K** | 355K | 427K | **+2.47x** ✅ | **+2.06x** ✅ | **#1 BEST** 🏆 |
| **Reads** | **2,207K** | 1,064K | 1,161K | **+2.07x** ✅ | **+1.90x** ✅ | **#1 BEST** 🏆 |
| **Mixed** | **718K** | 402K | 832K | **+1.79x** ✅ | **0.86x** ⚠️ | **#1 vs RocksDB** 🏆 |
| **Scans** | **19.6K** | 19.8K | 19.9K | 0.99x ≈ | 0.98x ≈ | **Competitive** 🎯 |

**Write Amplification**: 1.01x (4.82x better than traditional LSM) 🏆 **BEST-IN-CLASS**

### Achievement Summary

✅ **CRUSHING ROCKSDB**:
- Beat RocksDB on ALL 3 major workloads (1.79x-2.47x faster)
- Best-in-class: Writes (2.47x), Reads (2.07x), Write Amplification (4.82x better)
- Competitive scans (within 2% of RocksDB/fjall)

⚠️ **fjall Gap**:
- Only remaining issue: 14% behind fjall on mixed (718K vs 832K)
- **Mystery**: We're faster on BOTH pure workloads but slower on mixed!

**Latest Optimizations** (Nov 8):
- jemalloc allocator: +17-21% all workloads
- ArcSwap lock-free: +1-4%
- SIMD k-way merge: +3-4% reads
- LZ4 compression: +34.7% writes
- ALEX learned index: +55% reads

---

## Phase 10: Close fjall Gap (Nov 8-15, 2025) 🎯

**The Mystery**: fjall achieves >100% theoretical mixed performance!
```
fjall theoretical: (427K writes + 1,161K reads) / 2 = 794K
fjall actual: 832K (104.8% efficiency!)
```

This suggests:
- Read/write pipelining (operations overlap)
- Batch synergies (mixed triggers optimizations)
- Cache warming (writes benefit reads)

### Priority 1: Investigate fjall's Code (Days 1-2) 🔍

**Status**: ⏭️ **START NOW**

**Tasks**:
- [ ] Clone fjall repository (`git clone https://github.com/fjall-rs/fjall`)
- [ ] Map out their architecture (memtable, WAL, LSM tree, compaction)
- [ ] Analyze mixed workload code path
- [ ] Search for read/write pipelining (`rg "read.*write|batch|pipeline"`)
- [ ] Identify cache synergies
- [ ] Profile our mixed workload with samply
- [ ] Compare hot paths (ours vs theirs)
- [ ] Document findings in `ai/research/FJALL_MIXED_ANALYSIS.md`

**Questions to Answer**:
1. How do they achieve >100% theoretical mixed performance?
2. Do they pipeline reads while writes happen?
3. Do they batch operations differently in mixed vs pure workloads?
4. What's their cache strategy?

**Deliverable**: Specific optimizations to implement
**Timeline**: 1-2 days
**Potential**: +10-20% if we find and implement their techniques

---

### Priority 2: Create Large-Scale Benchmarks (Day 3) 📊

**Status**: Pending (after fjall investigation)

**Problem**: Our 100K benchmark is too small
- Dataset: ~100MB (100K * 1KB)
- Cache hit rate: ~95%+
- Working set fits entirely in cache
- **Result**: Doesn't stress rkyv or multi-tier caching

**Tasks**:
- [ ] Create `examples/large_benchmark.rs` (1M ops, 1GB dataset)
- [ ] Create `examples/stress_benchmark.rs` (10M ops, 10GB dataset)
- [ ] Implement Zipfian distribution (80/20 access pattern)
- [ ] Add configurable access patterns (uniform, zipfian, sequential)
- [ ] Measure cache hit rates explicitly
- [ ] Run baseline benchmarks (1M and 10M)
- [ ] Document in `ai/research/LARGE_SCALE_BENCHMARKS.md`

**Benchmark Configuration**:
```rust
// 1M ops benchmark
Operations: 1_000_000
Value size: 1024 bytes
Dataset: ~1GB
Distributions: uniform, zipfian(0.8), sequential
Expected cache hit rate: 60-80%

// 10M ops stress test
Operations: 10_000_000
Value size: 1024 bytes
Dataset: ~10GB
Distributions: uniform, zipfian(0.8)
Expected cache hit rate: 40-60%
```

**Success Criteria**:
- Cache hit rate <80% (shows cache pressure)
- Benchmarks complete in reasonable time (<10 min for 1M, <2 hours for 10M)
- Stable, reproducible results

**Timeline**: 1 day

---

### Priority 3: Test rkyv + Multi-Tier Cache at Scale (Day 4) 🧪

**Status**: Pending (after benchmarks exist)

**Test Plan**:
1. Run 1M baseline (document performance)
2. Implement rkyv for Block deserialization
3. Run 1M with rkyv (measure improvement)
4. Implement multi-tier cache (L1 decompressed + L3 compressed)
5. Run 1M with multi-tier cache (measure improvement)
6. Run 1M with both (measure combined)
7. Document results in `ai/research/RKYV_CACHE_EVALUATION.md`

**Expected Benefits** (at 1M+ scale):
- rkyv: +5-10% (7x faster deserialization on cache misses)
- Multi-tier cache: +8-12% (2x effective cache capacity)
- **Combined**: +13-22% potential

**Decision Criteria**:
| Optimization | Threshold | Action |
|--------------|-----------|--------|
| rkyv alone | >5% improvement | ✅ Implement |
| Multi-tier cache alone | >8% improvement | ✅ Implement |
| Combined | >15% improvement | ✅ Implement both |
| Either | <5% improvement | ❌ Skip |

**Timeline**: 1 day

---

### Priority 4: Implement Winning Optimizations (Day 5+)

**Status**: Pending (depends on findings)

**Potential Implementations**:

1. **fjall's techniques** (if found):
   - Read/write pipelining
   - Batch optimizations
   - Cache synergies
   - Expected: +10-20%
   - Timeline: Varies (depends on complexity)

2. **rkyv** (if validated at scale):
   - Zero-copy Block deserialization
   - Expected: +5-10%
   - Complexity: 3-5 days
   - Trade-offs: API complexity, larger serialized size

3. **Multi-tier cache** (if validated at scale):
   - L1: Decompressed blocks (1000 entries)
   - L3: Compressed blocks (10000 entries)
   - Expected: +8-12%
   - Complexity: 1-2 weeks

**Implementation Order** (based on impact/effort):
1. fjall techniques (highest impact, varies effort)
2. rkyv (medium impact, medium effort)
3. Multi-tier cache (high impact, high effort)

---

## Success Target

**Goal**: 718K → 850K+ mixed ops/sec (+18%)
- Beat fjall by ~5% (832K → 850K+)

**Optimistic Path**:
1. fjall techniques → +10-15%
2. rkyv (if validated) → +5-10%
3. Multi-tier cache (if validated) → +8-12%
4. **Cumulative**: +23-37% → 883K-984K ops/sec 🎯 **CRUSH FJALL**

**Conservative Path**:
1. fjall techniques → +5-10%
2. One of rkyv or multi-tier cache → +5-10%
3. **Cumulative**: +10-20% → 790K-862K ops/sec 🎯 **BEAT FJALL**

---

## Deferred Optimizations ⏸️

### tokio-uring (Linux I/O)
- **Status**: Deferred until profiling shows I/O >20% of time
- **Effort**: 4 days, 300-500 LOC, NOT drop-in
- **Potential**: +20-50% I/O throughput (Linux only)
- **Blocker**: Need profiling data + cross-platform complexity

### mmap for Read-Only SSTables
- **Status**: Skipped (doesn't help compressed blocks)
- **Benefit**: +2-5% (bloom/index only)
- **Reason**: Main data is LZ4 compressed, mmap doesn't help

---

## Completed Optimizations ✅

### Phase 9: Advanced Optimizations (Nov 8, 2025)
- ✅ jemalloc allocator (+17-21% all workloads) 🔥
- ✅ ArcSwap lock-free structures (+1-4%)
- ✅ SIMD k-way merge (+3-4% reads)
- ✅ Individual optimization testing methodology
- ✅ Allocator comparison (jemalloc vs mimalloc vs system)

### Phase 8: SOTA Library Implementation (Nov 8, 2025)
- ✅ LZ4 block compression (+34.7% writes) 🔥
- ✅ foldhash (2x faster hashing)
- ✅ varint-rs (space-efficient encoding)
- ✅ quick_cache (lock-free SSTable cache)

### Phase 7: ALEX Learned Index (Nov 7, 2025)
- ✅ O(log error) lower_bound (+55% reads) 🔥
- ✅ Exponential search around model prediction

### Earlier Phases
- ✅ Partitioned memtables (16 partitions)
- ✅ Lock-free WAL
- ✅ Decompressed block cache
- ✅ Dostoevsky compaction
- ✅ WiscKey vLog (write amp: 1.01x)

---

## References

**Current State**:
- `ai/STATUS.md` - Complete current state and next phase plan
- `ai/research/COMPREHENSIVE_INVESTIGATION.md` - Full investigation of fjall gap + optimizations
- `ai/research/ALLOCATOR_ANALYSIS.md` - jemalloc vs mimalloc comparison
- `ai/research/ADVANCED_OPTIMIZATIONS.md` - rkyv, caching, io_uring, mmap analysis

**Design**:
- `ai/design/BLOCK_SSTABLE_FORMAT.md` - V3 format with LZ4 + varint
- `ai/DECISIONS.md` - All architecture decisions

**Performance**:
- Crushing RocksDB: 1.79x-2.47x across all major workloads ✅
- Gap to fjall: 14% on mixed workload (718K vs 832K) - **investigating**
- Write amplification: 4.82x better than traditional LSM ✅

---

**Status**: 🔍 **Investigation Phase** - Understanding fjall's mixed workload advantage
**Next Action**: Clone fjall, analyze their code, profile our mixed workload
**Updated**: November 8, 2025 - After jemalloc allocator optimization

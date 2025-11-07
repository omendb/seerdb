# TODO - seerdb

**Last Updated**: November 7, 2025
**Current Focus**: SOTA Algorithmic Optimizations

---

## Phase 9: SOTA Algorithmic Optimizations (4-6 weeks)

**Goal**: 218K → 500K writes (+129%) via research-backed algorithms
**Status**: Planning complete, ready to implement

### Current Performance Gap

| Workload | seerdb | fjall | Gap |
|----------|--------|-------|-----|
| Writes (100K) | 218K | 423K | 2x slower ❌ |
| Writes (1M) | 341K | ~420K | 1.23x slower |
| Writes (1M, BG flush) | 473K | ~420K | 1.13x faster ✅ |
| Mixed (1M) | 420K | ~550K | 1.31x slower |

**Target**: Beat fjall across all workloads

---

## Priority 1: Prefix Compression (1-2 weeks) ⭐⭐⭐ ✅ COMPLETE

**Expected**: +15-25% writes, 30-50% space reduction
**Research**: Standard in LevelDB, RocksDB, PebblesDB
**Complexity**: Medium
**Status**: ✅ Complete (commit 241c6d2)

**Implementation** (src/sstable/block.rs):
- [x] Modify BlockBuilder to track last_key
- [x] Calculate shared prefix length for each entry
- [x] Encode: [prefix_len][suffix_len][suffix][value_len][value]
- [x] Update BlockIterator to reconstruct full keys
- [x] Add restart points every N entries (for binary search)
- [x] Benchmark space reduction and throughput
- [x] Run all 126 tests to verify correctness

**Results**:
- Space reduction: **31.1%** for typical workloads ✅
- Write throughput: **218K → 218K** ops/sec (0% change, neutral) ✅
- All tests pass ✅

**Key Findings**:
- 31% space savings across all workload types
- Zero throughput regression (encoding/decoding cost negligible)
- Even random keys benefit from format change (u32+u32 → u16+u16)

---

## Priority 2: SIMD Key Comparisons (3-5 days) ⭐⭐

**Expected**: +5-15% overall throughput
**Research**: Standard optimization in high-performance systems
**Complexity**: Medium (platform-specific)

**Implementation** (src/memtable/mod.rs, src/sstable/block.rs):
- [ ] Add SIMD feature flag to Cargo.toml
- [ ] Implement compare_keys_simd() with SSE2/AVX2
- [ ] Use in skiplist key comparisons
- [ ] Use in block binary search
- [ ] Add ARM NEON variant for M-series Macs
- [ ] Benchmark: skiplist insert, binary search
- [ ] Fallback to scalar for non-SIMD platforms

**Success Criteria**:
- Write throughput: 260K → 285K ops/sec (+10%)
- Read throughput: +5-10%
- All tests pass on x86_64, arm64, fallback

---

## Priority 3: Partitioned Memtables (1-2 weeks) ⭐⭐

**Expected**: +25-40% writes on multi-core
**Research**: Tucana (2020), FASTER (2018)
**Complexity**: High (affects all code paths)

**Implementation** (src/db.rs, src/memtable/mod.rs):
- [ ] Add NUM_PARTITIONS const (16 partitions)
- [ ] Change memtable: Arc<Mutex<Memtable>> → [Arc<Mutex<Memtable>>; 16]
- [ ] Implement partition_for_key() using xxhash
- [ ] Update put() to lock only one partition
- [ ] Update get() to check correct partition
- [ ] Update flush() to merge all partitions
- [ ] Update range() to query all partitions (k-way merge)
- [ ] Benchmark on multi-core system
- [ ] Run all tests to verify correctness

**Success Criteria**:
- Write throughput: 285K → 380K ops/sec (+33%)
- Lock contention reduced by 16x
- All tests pass

---

## Priority 4: Dostoevsky LSM Tuning (1-2 weeks) ⭐⭐⭐

**Expected**: +20-30% writes (reduce write amp)
**Research**: Dayan et al., Harvard 2018
**Complexity**: Medium

**Implementation** (src/compaction/mod.rs):
- [ ] Add CompactionStrategy enum (Leveling, LazyLeveling, Tiering)
- [ ] Implement lazy leveling: L0 overlapping, L1+ single run
- [ ] Add workload detection (read/write ratio)
- [ ] Auto-select strategy based on workload:
  - Write-heavy (<30% reads): LazyLeveling, ratio=4
  - Read-heavy (>70% reads): Leveling, ratio=10
  - Balanced: LazyLeveling, ratio=7
- [ ] Measure write amplification before/after
- [ ] Benchmark on different workloads
- [ ] Update DBOptions to expose strategy

**Success Criteria**:
- Write amplification: 1.01x → 0.7x (-30%)
- Write throughput: 380K → 480K ops/sec (+26%)
- All tests pass

---

## Priority 5: Lock-Free Memtable Access (3-5 days) ⭐

**Expected**: +10-20% writes
**Research**: Fraser 2004
**Complexity**: High (unsafe code)

**Implementation** (src/db.rs):
- [ ] Change memtable: Arc<Mutex<Memtable>> → AtomicPtr<Memtable>
- [ ] Implement atomic CAS for memtable swap
- [ ] Careful memory management (Box::into_raw, Box::from_raw)
- [ ] Update put() to use atomic load/store
- [ ] Update get() to use atomic load
- [ ] Update flush() to use atomic CAS
- [ ] Extensive testing (race conditions, memory leaks)
- [ ] Valgrind/MIRI testing

**Success Criteria**:
- Write throughput: 480K → 520K ops/sec (+8%)
- Zero mutex overhead
- All tests pass, no memory leaks

---

## Priority 6: Bloom Filter SIMD (2-3 days) ⭐

**Expected**: +3-5% overall
**Research**: Standard SIMD optimization
**Complexity**: Low

**Implementation** (src/sstable/bloom.rs):
- [ ] Implement bloom_check_simd() with AVX2
- [ ] Check 4 hash positions simultaneously
- [ ] Use _mm_or_si128 for parallel bit checks
- [ ] Benchmark bloom filter lookup time
- [ ] Add to both learned and traditional bloom

**Success Criteria**:
- Bloom filter lookups: 2-3x faster
- Overall throughput: +3-5%
- All tests pass

---

## Timeline

### Week 1-2: Prefix Compression + SIMD Keys
- Days 1-7: Implement prefix compression
- Days 8-10: Implement SIMD key comparisons
- Days 11-12: Benchmark and validate
- **Expected**: 218K → 285K writes (+31%)

### Week 3-4: Partitioned Memtables
- Days 13-19: Implement partitioned memtables
- Days 20-21: Benchmark and validate
- **Expected**: 285K → 380K writes (+33%)

### Week 5: Dostoevsky LSM Tuning
- Days 22-26: Implement lazy leveling
- Days 27-28: Benchmark and validate
- **Expected**: 380K → 480K writes (+26%)

### Week 6: Lock-Free + Bloom SIMD
- Days 29-32: Lock-free memtable access
- Days 33-34: Bloom filter SIMD
- Days 35-36: Final benchmarks
- **Expected**: 480K → 520K writes (+8%)

**Total**: 218K → 520K writes (+139%, beat fjall's 423K)

---

## Success Metrics

### Phase 9 Complete
- ✅ Writes: 520K+ ops/sec (1.23x fjall)
- ✅ Write amplification: 0.7x (maintained or improved)
- ✅ All 126 tests passing
- ✅ Prefix compression: 30-50% space reduction
- ✅ SIMD: 5-15% overall improvement
- ✅ Partitions: 16x less lock contention
- ✅ Dostoevsky: Workload-aware compaction
- ✅ Lock-free: Zero mutex overhead

### Validation
Each optimization must:
1. Have research paper backing
2. Show measurable improvement (>10%)
3. Pass all 126 tests
4. Benchmark vs baseline

**No parameter tweaking without algorithmic justification**

---

## Not Implementing (Parameter Tweaking)

❌ Disable vLog by default - hiding features
❌ Change memtable size - parameter tuning
❌ Adjust batch sizes - already optimal
❌ Change level ratios without Dostoevsky math

---

## Phase 9 Tasks (Current Work)

### Immediate (This Session)
- [x] Update ai/STATUS.md with background flush findings
- [x] Update ai/TODO.md with SOTA optimization plan
- [ ] Update ai/DECISIONS.md with background flush decision
- [ ] Start implementing Priority 1: Prefix Compression

**Next Action**: Implement prefix compression in BlockBuilder
**Timeline**: 1-2 weeks for first optimization
**Priority**: 🔴 HIGH - SOTA algorithmic improvements

---

**References**:
- SOTA_ALGORITHMIC_IMPROVEMENTS.md - Detailed implementation plans
- PERFORMANCE_FINDINGS.md - Large benchmark results
- BACKGROUND_FLUSH_IMPLEMENTATION.md - Background flush details

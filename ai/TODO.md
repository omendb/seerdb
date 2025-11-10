# TODO - seerdb

**Last Updated**: November 10, 2025
**Current Sprint**: Week 5-6 Testing Phase (Nov 10-23, 2025)
**Goal**: Achieve 80%+ test coverage for 0.0.1 release

---

## Current Status (Nov 10, 2025)

### ✅ All Critical Bugs Fixed!

**Major Milestones**:
- ✅ All 9 critical bugs resolved
- ✅ All tests passing (100% pass rate)
- ✅ Crash recovery validated
- ✅ Compaction safety (delayed deletion queue)
- ✅ WAL recovery race fixed
- ✅ Batch API atomicity (single WAL record)
- ✅ Checksum validation on all reads
- ✅ MVCC decision: Defer to 0.0.2+ (Read Committed sufficient for vectors)

### Performance Achievement

| Workload | seerdb | RocksDB | fjall | vs RocksDB | vs fjall | Status |
|----------|--------|---------|-------|------------|----------|--------|
| **Writes** | **878K** | 355K | 427K | **+2.47x** ✅ | **+2.06x** ✅ | **#1 BEST** 🏆 |
| **Reads** | **2,207K** | 1,064K | 1,161K | **+2.07x** ✅ | **+1.90x** ✅ | **#1 BEST** 🏆 |
| **Mixed** | **718K** | 402K | 832K | **+1.79x** ✅ | **0.86x** ⚠️ | **#1 vs RocksDB** 🏆 |
| **Scans** | **19.6K** | 19.8K | 19.9K | 0.99x ≈ | 0.98x ≈ | **Competitive** 🎯 |

**Write Amplification**: 1.01x (4.82x better than traditional LSM) 🏆 **BEST-IN-CLASS**

**Verdict**: fjall mixed gap (14%) is acceptable. We're 1.79x faster than RocksDB (industry standard). Focus on correctness over marginal mixed workload optimization.

---

## Week 5-6: Testing Phase (Nov 10-23, 2025) 🧪

**Goal**: Achieve 80%+ code coverage for 0.0.1 release

**Current Coverage**: ~15% (estimated)
**Target**: 80%+ overall, 90%+ for critical modules (WAL, memtable, SSTable, compaction)

### Detailed Plan

See `ai/TESTING_STRATEGY.md` for comprehensive testing roadmap.

### Phase 1: Critical Coverage Gaps (Days 1-3)

**Goal**: +20% coverage (15% → 35%)

#### Day 1: ALEX Learned Index Tests (~300 LOC, 15 tests)
**Status**: ⏭️ **START NOW**

**Missing Coverage**:
- [ ] Node split logic (when node exceeds capacity)
- [ ] Node merge logic (when node underflows)
- [ ] Multi-level tree traversal (root → inner → leaf)
- [ ] Bulk loading (initial index construction)
- [ ] Error prediction bounds (validate O(log error) guarantee)
- [ ] Concurrent modifications (thread safety)

**Target**: +5% coverage
**File**: Create `tests/alex_learned_index_tests.rs`

#### Day 2: VLog Tests (~400 LOC, 20 tests)
**Status**: Pending

**Missing Coverage**:
- [ ] VLog corruption detection (checksum validation)
- [ ] VLog truncation handling (partial writes)
- [ ] VLog header validation (magic number, version)
- [ ] VLog rotation (when file exceeds size limit)
- [ ] VLog concurrent reads (multiple readers)

**Target**: +5% coverage
**File**: Create `tests/vlog_tests.rs`

#### Day 3: Compaction Tests (~300 LOC, 15 tests)
**Status**: Pending

**Missing Coverage**:
- [ ] Multi-level cascading compaction (L0→L1→L2→...)
- [ ] Size ratio enforcement (10x between levels)
- [ ] Overlapping key ranges (L0 → L1 merges)
- [ ] Compaction throttling (when too many L0 files)
- [ ] Compaction cancellation (on DB close)

**Target**: +5% coverage
**File**: Extend `tests/compaction_correctness_tests.rs`

### Phase 2: Medium Priority (Days 4-5)

**Goal**: +10% coverage (35% → 45%)

#### Day 4: SSTable + WAL Tests (~400 LOC, 20 tests)
- [ ] Prefix compression edge cases (empty prefix, full key prefix)
- [ ] Varint decoding errors (truncated, invalid)
- [ ] Block corruption (CRC mismatch, invalid format)
- [ ] WAL partial record writes (truncated at end)
- [ ] WAL recovery with batch records (batch atomicity)

**Target**: +5% coverage

#### Day 5: Iterator + Memtable Tests (~350 LOC, 18 tests)
- [ ] Iterator edge cases (empty range, single key)
- [ ] Memtable partition selection (key distribution)
- [ ] Memtable concurrent reads/writes

**Target**: +5% coverage

### Phase 3: Polish (Day 6)

**Goal**: +5% coverage (45% → 50%+)

- [ ] Remaining gaps identified by coverage tool
- [ ] Edge cases from code review
- [ ] Integration test scenarios

**Target**: +5% coverage

### Phase 4: Sanitizer Runs (Days 7-8)

#### Day 7: Address Sanitizer (ASAN)
```bash
RUSTFLAGS="-Z sanitizer=address" cargo test --target x86_64-apple-darwin
```
**Detects**: Use-after-free, buffer overflows, memory leaks

#### Day 8: Thread Sanitizer (TSAN)
```bash
RUSTFLAGS="-Z sanitizer=thread" cargo test --target x86_64-apple-darwin
```
**Detects**: Data races, deadlocks

---

## Deferred to 0.0.2+ (Post-Release) ⏸️

### Performance Optimizations
- **fjall mixed gap** (718K vs 832K) - Already 1.79x faster than RocksDB ✅
- **rkyv zero-copy** - Only +3% benefit, high complexity
- **Multi-tier caching** - Needs production workload data
- **tokio-uring** (Linux I/O) - Deferred until profiling shows I/O >20% of time

### Advanced Features
- **MVCC/Snapshot API** - Deferred (Read Committed sufficient for vectors)
- **VLog GC** - Deferred (GC not implemented yet, will be done correctly)
- **Learned bloom filters** - Prototype exists, needs validation

---

## Completed Optimizations ✅

### Bug Fixes (Nov 9-10, 2025)
- ✅ All 9 critical bugs resolved
- ✅ Block cache unbounded (fixed with quick_cache LRU, 10K blocks, ~40MB limit)
- ✅ Batch API atomicity (single WAL batch record, atomic recovery)
- ✅ Checksums (SSTable footer validated on read)
- ✅ Magic numbers (WAL/VLog have magic + version)
- ✅ Iterator invalidation (memtables collected before SSTables)
- ✅ Compaction live key deletion (delayed deletion queue)
- ✅ WAL recovery race (barrier synchronization + file cursor seek)
- ✅ Tombstone handling (SSTable.contains() distinguishes tombstone from miss)

### Performance Optimizations (Nov 7-8, 2025)
- ✅ jemalloc allocator (+17-21% all workloads) 🔥
- ✅ ArcSwap lock-free structures (+1-4%)
- ✅ SIMD k-way merge (+3-4% reads)
- ✅ LZ4 block compression (+34.7% writes) 🔥
- ✅ foldhash (2x faster hashing)
- ✅ varint-rs (space-efficient encoding)
- ✅ quick_cache (lock-free SSTable cache)
- ✅ ALEX learned index (+55% reads) 🔥

### Core Features (Oct-Nov 2025)
- ✅ Partitioned memtables (16 partitions)
- ✅ Lock-free WAL
- ✅ Decompressed block cache
- ✅ Dostoevsky compaction
- ✅ WiscKey vLog (write amp: 1.01x)

---

## References

**Planning**:
- `ai/TESTING_STRATEGY.md` - Comprehensive testing roadmap (80%+ coverage)
- `ai/PRODUCTION_READINESS.md` - 8-week roadmap to 0.0.1
- `ai/BUGS_AND_EDGE_CASES.md` - All known bugs (all resolved!)

**Current State**:
- `ai/CURRENT_STATE.md` - TL;DR current status
- `ai/STATUS.md` - Detailed performance history

**Design**:
- `ai/DECISIONS.md` - All architecture decisions (including MVCC deferral)
- `ai/design/BLOCK_SSTABLE_FORMAT.md` - V3 format with LZ4 + varint

**Research**:
- `ai/research/LSM_MVCC_CONCURRENCY_RESEARCH.md` - MVCC analysis (800+ lines)
- `ai/research/COMPREHENSIVE_INVESTIGATION.md` - fjall gap investigation
- `ai/research/ALLOCATOR_ANALYSIS.md` - jemalloc vs mimalloc comparison

---

**Status**: 🧪 **Testing Phase (Week 5-6)** - Achieving 80%+ test coverage
**Next Action**: Implement ALEX learned index tests (~300 LOC, 15 tests)
**Updated**: November 10, 2025 - After strategic planning and MVCC decision

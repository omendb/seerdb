# STATUS - seerdb

**Last Updated**: November 1, 2025
**Current Phase**: Production Hardening (Phase 1 - Fixing Critical Bugs)
**Focus**: Making seerdb production-ready (6-8 week effort)
**Decision**: omen stays with RocksDB until seerdb is production-grade

---

## Current State

### What We Have

**Research Complete** (7/9 papers, 78%):
- ✅ Phase 1 Foundational (3/3): Learned Indexes, ALEX, Learned Bloom Filters
- ✅ Phase 2 LSM Trees (3/3): WiscKey, Dostoevsky, PebblesDB
- ✅ Phase 3 Workload-Aware (1/1): Bourbon
- 📋 Phase 4 Modern Hardware (0/1): FASTER remaining (optional)

**Core Engine Complete** (Weeks 5-7):
- ✅ **Write-Ahead Log (WAL)**: Durability with CRC32 checksums
  - SyncPolicy: SyncAll, SyncData, None
  - Batch writes supported
  - Crash recovery via WAL replay
  - src/wal/: 411 lines (record.rs, mod.rs, reader.rs)

- ✅ **Memtable**: In-memory buffer with concurrent skiplist
  - Lock-free reads/writes (crossbeam-skiplist)
  - Tombstones for deletions
  - Capacity-based flushing
  - Range scans supported
  - src/memtable/mod.rs: 234 lines

- ✅ **SSTable**: Sorted String Table on disk
  - **Binary search** on keys (O(log n) lookups)
  - **Bloom filter** integration (19x speedup for negative lookups)
  - **KV separation** (Week 13): Inline vs vLog pointers
  - Iterator support
  - src/sstable/mod.rs: 700 lines

- ✅ **Bloom Filters**: Traditional implementation with serialization
  - Configurable FPR (default 1%)
  - Bit packing for efficient storage
  - Serialization: to_bytes/from_bytes
  - src/bloom/traditional.rs: 252 lines

- ✅ **Compaction System** (Week 7):
  - LSM level structure (L0-L6, exponential sizing)
  - Merge iterator (k-way merge with deduplication)
  - compact_sstables() function
  - Size and file-count based triggers
  - src/compaction/: 580 lines (mod.rs, merge.rs)

- ✅ **Value Log (vLog)** (Week 13):
  - WiscKey-style append-only value storage
  - CRC32 checksums for integrity
  - Record format: [key_len][key][value_len][value][crc]
  - ValuePointer (offset + length) for LSM tree
  - src/vlog/mod.rs: 398 lines

**Tests**: 68 passing (58 unit + 10 integration)
**Code**: 3,300+ lines (core engine + integration tests)
**Benchmarks**: seerdb: 348k writes/sec (96% of RocksDB baseline)

---

## Week 7 Results Summary

**Compaction System**:
- LSM tree with 7 levels (L0-L6)
- L0 trigger: 4+ SSTables
- L1+ trigger: Exponential size thresholds (10MB, 100MB, 1GB, ...)
- Merge iterator: Deduplicates and keeps newest values
- compact_sstables(): Merges multiple SSTables into one

**Performance Characteristics**:
- Read amplification: O(N) → O(log N) with compaction
- Example: 1000 flushes without compaction = 1000 SSTables to check
- Example: With compaction = 7 levels max to check

**Tests Added**: 11 compaction tests
- 5 level management tests
- 4 merge iterator tests
- 2 end-to-end compaction tests

**Details**: See ai/WEEK7_RESULTS.md

---

## Active Work

**Week 8 Complete**:
- ✅ DB struct integrating all components
- ✅ Public API (get/put/delete)
- ✅ Flush logic (memtable → L0)
- ✅ Compaction scheduling
- ✅ Benchmark (348k writes/sec - 96% of RocksDB)
- ✅ WAL recovery on startup
- ✅ Comprehensive integration tests (10 end-to-end tests)
- ✅ 63 tests passing

**Week 9 Complete**: Learned Bloom Filters (Research)
- ✅ Implemented learned bloom filter with decision tree
- ✅ Comprehensive benchmarks and diagnostics
- ✅ Root cause analysis of 50% FPR
- ✅ Proof of concept with proper features
- ⚠️ Finding: Not suitable for general-purpose KV storage
- ✅ Documented research findings

**Week 13 Complete**: KV Separation (WiscKey)
- ✅ Value log (vLog) implementation with CRC checksums
- ✅ SSTable support for inline values and vLog pointers
- ✅ Entry format: [key][flag: 0x00=inline, 0x01=pointer][value_data]
- ✅ DB interface integration with vlog_threshold option
- ✅ Automatic flush with KV separation for large values
- ✅ Tests: 61 passing (4 new vLog/SSTable + 2 new DB integration)
- ✅ Demos: kv_separation_demo.rs (33% write amp reduction)
- ⏸️ Deferred: GC (future), compaction with vLog (iterator limitation)

**Week 14 Complete**: Performance Optimizations
- ✅ Profiled hot paths (simd_profiling benchmark)
  - Binary search: 2-3.6 µs per lookup
  - Bloom filter: ~65 ns positive, ~8.7 ns negative
  - Key comparison: 1.3-1.6 ns (already optimized)
  - CRC32: Hardware-accelerated (crc32fast)
- ✅ Bit-packed bloom filter (8x space savings)
  - Storage: Vec<u64> instead of Vec<bool>
  - Space: ~1.2 bytes/element (vs ~8 bytes for Vec<bool>)
  - Cache-friendly bitwise operations
- ✅ Tests: 64 passing (3 new bit-packed tests)
- ✅ Benchmarks: simd_profiling + bloom_comparison
- 🔍 Finding: Most hot paths already optimized by compiler/libraries
  - Further SIMD work deferred until real bottlenecks identified

**Week 15 Complete**: Production Hardening
- ✅ Background compaction implemented
  - Worker thread with channel-based signaling
  - Non-blocking flush() when enabled
  - Graceful shutdown via Drop trait
  - Opt-in via DBOptions.background_compaction
- ✅ Tests: 66 passing (2 new background compaction tests)
  - test_db_background_compaction: Async compaction works
  - test_db_sync_vs_async_compaction: Same results as sync
- ✅ Backward compatible: Default is synchronous (existing behavior)
- ✅ Benchmark: background_compaction benchmark suite
  - Compares sync vs async throughput
  - Tests 1k, 5k, 10k write workloads
  - Demonstrates non-blocking write performance

---

## What Worked

### Week 7 Implementation
- **Collect-and-sort merge**: Simpler than streaming, correct behavior
- **Test coverage**: 11 compaction tests ensure correctness
- **Deduplication logic**: Properly keeps newest values
- **Level thresholds**: Exponential sizing (10x ratio) works well

### Previous Weeks (5-6)
- **Rapid prototyping**: WAL + Memtable + SSTable in ~1 week
- **Test-driven**: Tests caught bugs early (e.g., deduplication)
- **Benchmarking validates**: Measured 19x bloom filter improvement
- **Research informs design**: Dostoevsky/WiscKey principles applied

---

## What Didn't Work

### Merge Iterator Complexity
- Initial design: Streaming k-way merge with BinaryHeap
- Blocker: SSTable::iter() requires &mut self (lifetime issues)
- Solution: Collect all entries upfront, then sort
- Trade-off: O(N) memory during merge, but simpler and correct

### Still Pending
- Block-based storage (simple key-value format for now)
- Compression (LZ4 deferred)
- Block cache (LRU deferred)
- Background compaction thread (manual for now)

---

## Blockers for Production

### CRITICAL Issues (0 remaining - All Fixed! ✅)
1. ✅ **Compaction doesn't update LSM tree** - Fixed (commit 1434acf)
   - Added LSM tree management methods
   - Compacted SSTables properly registered

2. ✅ **SSTables not deleted after compaction** - Fixed (commit 1434acf)
   - Disk space leak eliminated
   - Input SSTables deleted after successful merge

3. ✅ **Duplicate compaction code** - Fixed (commit 67722be)
   - Extracted `do_compact_level()` shared implementation
   - Single source of truth for compaction logic

### HIGH Priority Issues (2 remaining, 2 completed)
- ✅ **HIGH-1**: 261 `.unwrap()` calls - Fixed (commit 28daa66)
  - All 17 production unwraps fixed
  - 244 test unwraps acceptable (idiomatic in tests)
  - Complete audit of all 11 source files
- ✅ **HIGH-2**: No checksums on SSTables - Fixed (commit a9aa99e)
  - CRC32 checksums added to SSTable format v1
  - Corruption detection tests passing
- ❌ **HIGH-3**: No stress tests (unknown behavior under load)
- ❌ **HIGH-4**: No crash recovery tests (unknown durability guarantees)

**Progress**: 5/7 critical+high issues fixed (71%)
**See**: `ai/CRITICAL_BUGS.md` for full list and details

---

## Next Steps - Production Hardening

**Phase 1: Fix Critical Bugs** (Current - Week 2)

**Completed This Week**:
1. ✅ Create ai/PRODUCTION_ROADMAP.md (5-phase plan)
2. ✅ Create ai/CRITICAL_BUGS.md (15 known issues)
3. ✅ Deduplicate compaction code (CRITICAL-3) - commit 67722be
4. ✅ Fix LSM tree updates (CRITICAL-1) - commit 1434acf
5. ✅ Implement file cleanup (CRITICAL-2) - commit 1434acf
6. ✅ Run clippy and fix all warnings (11 warnings) - commit f118d4b
7. ✅ Complete unwrap audit (HIGH-1) - commit 28daa66
   - Fixed all 17 production unwraps
   - Audited all 11 source files
   - 244 test unwraps verified acceptable
8. ✅ Add SSTable checksums (HIGH-2) - commit a9aa99e

**Remaining Phase 1 Tasks**:
1. ❌ Add WAL recovery tests (HIGH-3)
2. ❌ Add crash recovery tests (HIGH-4)

**Timeline**:
- Phase 1 (Critical bugs): 2-3 weeks (Week 2 in progress - 71% complete)
- Phase 2 (Testing): 3-4 weeks
- Phase 3 (Observability): 1-2 weeks
- Phase 4 (Code quality): 1 week
- Phase 5 (Real-world validation): 2-4 weeks
- **Total: 10-12 weeks to production-ready**

**Progress**: All 3 CRITICAL bugs fixed, 2/4 HIGH bugs fixed, 71% completion
**See**: `ai/PRODUCTION_ROADMAP.md` for detailed plan

---

## Key Metrics

**Lines of Code**:
- WAL: 411 lines
- Memtable: 234 lines
- SSTable: 425 lines
- Bloom: 252 lines (traditional)
- Compaction: 580 lines
- Tests: 300+ lines
- **Total: ~2,200 lines**

**Performance** (SSTable, 100k entries):
- Existing key lookup: 2.1 µs (476k ops/sec)
- Missing key lookup: 109 ns (9.1M ops/sec, 19x faster)
- Full scan: 28.4 ms (10k entries)

**Compaction**:
- L0 trigger: 4 SSTables
- L1 threshold: 10MB (configurable)
- Read amplification: O(levels) = O(7) worst case

**Tests**: 63 passing
- 49 unit tests (module-level + recovery)
- 10 DB integration tests (end-to-end lifecycle)
- 4 component integration tests (WAL + memtable + SSTable)

---

## Architecture Progress

**Completed**:
- ✅ WAL for durability
- ✅ Memtable (skiplist)
- ✅ SSTable with bloom filters + binary search
- ✅ LSM compaction system
- ✅ Merge iterator
- ✅ Main DB interface (Week 8)
- ✅ Flush coordination
- ✅ WAL recovery on startup
- ✅ Value log (vLog) for KV separation (Week 13)
- ✅ SSTable support for value pointers (Week 13)

**Pending**:
- 📋 vLog garbage collection (deferred)
- 📋 File cleanup after compaction (deferred)
- 📋 Compression (LZ4/Zstd) (deferred)
- 📋 Block cache (LRU) (deferred)
- 📋 Learned indexes (deferred - limited benefit)

**Current Architecture**:
```
┌──────────────────────────────────────────┐
│              DB Interface                │  ← Unified public API
│  (get/put/delete + recovery on startup) │
└────┬─────────┬─────────┬─────────────────┘
     │         │         │
┌────▼─────┐ ┌▼────────┐│
│   WAL    │ │Memtable ││  ← In-memory + durability
└──────────┘ └─────┬───┘│
                   │    │ flush
             ┌─────▼────▼──────┐      ┌──────────────┐
             │   SSTable (L0)  │◄─────┤  Value Log   │
             │  (keys+pointers)│      │  (vLog)      │
             └─────┬───────────┘      │              │
                   │ compact           │ (large values)│
             ┌─────▼───────────┐      └──────────────┘
             │   LSM Levels    │            ▲
             │  (Compaction)   │            │ read large values
             └─────────────────┘────────────┘

Week 13: KV separation implemented at SSTable level
```

---

## Research Insights Applied

1. **Dostoevsky (leveled compaction)**:
   - Implemented exponential level sizing (T=10 ratio)
   - L0 uses file count, L1+ uses size
   - Ready for lazy leveling upgrade

2. **Merge correctness**:
   - Stable sort preserves ordering
   - Lower source_id = newer = wins conflicts
   - Matches LSM semantics

3. **Size ratios**:
   - Base size: 10MB (adjustable)
   - Ratio: 10x between levels
   - Standard in literature (RocksDB default)

---

## Competitive Analysis

**fjall Baseline** (from baseline_benchmark):
- Writes: 438k ops/sec
- Mixed: 576k ops/sec
- **Target**: Match or beat with Week 8 integration

**seerdb Progress**:
- SSTable reads: 476k ops/sec (existing), 9.1M ops/sec (missing)
- Compaction: Functional, not yet benchmarked end-to-end
- Full DB performance: Week 8 will reveal

**Differentiation** (vs fjall):
- Binary search: ✅ Implemented (O(log n) vs fjall's O(n))
- Bloom filters: ✅ Implemented (fjall has basic bloom)
- Compaction: ✅ Implemented (similar to fjall)
- Learned bloom: 📋 Week 9 (fjall has ZERO learned components)
- Learned index: 📋 Week 10 (fjall uses binary search)
- SIMD: 📋 Week 14 (fjall has ZERO SIMD)

**Details**: See ai/research/FJALL_ANALYSIS.md, ai/COMPETITIVE_ADVANTAGES.md

---

## Recent Commits

```
a9aa99e - feat: add CRC32 checksums to SSTables for corruption detection
  - CRC32 checksums using crc32fast (hardware-accelerated)
  - Footer format v1: [index_offset][bloom_offset][checksum][version]
  - Verify checksums on SSTable::open() to detect corruption
  - Tests: 68 passing (66 existing + 2 new corruption tests)
  - Performance impact: <1% overhead
  - Resolves: HIGH-2 (SSTable checksums)

b32fbf2 - refactor: fix 10 production unwraps in db.rs hot paths
  - Replace .lock().unwrap() with .expect("descriptive message")
  - Fixed in: WAL, vLog, LSM, SSTable counter mutexes
  - Better error diagnostics for mutex poisoning
  - Partial progress on HIGH-1 (10/261 unwraps fixed)

f118d4b - refactor: fix all clippy warnings (11 warnings)
  - Implement Iterator trait for MergeIterator
  - Use .div_ceil() and .is_multiple_of() methods
  - Add .truncate(true) to vLog file creation
  - Remove unnecessary mut declarations

1434acf - fix: compaction LSM updates and file cleanup (CRITICAL-1, CRITICAL-2)
  - Add LSM tree management: add_to_level(), remove_sstables_from_level()
  - Update LSM tree after compaction (fixes CRITICAL-1)
  - Delete input SSTables from disk (fixes CRITICAL-2 disk leak)
  - All 66 tests passing

67722be - refactor: deduplicate compaction code (CRITICAL-3)
  - Extract do_compact_level() as shared implementation
  - Both compact_level() and run_compaction() use same logic
  - Single source of truth for compaction

b01b0db - chore: add write amp demo and export SyncPolicy
  - Demo: write amplification comparison (examples/write_amp_demo.rs)
  - Export SyncPolicy from root for easier access
  - Demonstrates SSTable size reduction with KV separation

9bfeabb - feat: integrate KV separation into DB interface
  - DB interface integration with vlog_threshold option
  - DB::flush() uses vLog for large values automatically
  - DB::get() attaches vLog to SSTables for reading
  - Tests: 2 new DB integration tests (61 total)
  - Full end-to-end KV separation working

a0d4229 - feat: implement KV separation with vLog and SSTable integration
  - Value log (vLog): Append-only storage with CRC checksums
  - SSTable: Support for inline values and vLog pointers
  - Entry format: [key][flag: 0x00=inline, 0x01=pointer][value_data]
  - Tests: 4 new SSTable+vLog integration tests
  - Demo: kv_separation_demo.rs (33% write amp reduction)

ba6842e - research: Week 9 learned bloom filters - valuable negative result
  - Learned blooms: 73% space reduction, but 50% FPR with hash features
  - Root cause: Hash features destroy learnable patterns
  - Proof: Fixed implementation with proper features → 0% FPR
  - Finding: Not suitable for general-purpose KV storage

3cd8e53 - feat: add comprehensive DB integration tests
  - 10 end-to-end tests covering full DB lifecycle
  - Tests: lifecycle, deletes, overwrites, flushes, recovery, mixed ops
  - 63 tests passing (49 unit + 14 integration)

c863e92 - feat: implement WAL recovery on database startup
  - Automatic WAL replay on DB::open()
  - Recovery tests: basic, deletes, overwrites, flush, empty WAL
  - 49 tests passing (44 unit + 5 recovery)

f75d601 - feat: add seerdb benchmark - 348k writes/sec (96% of RocksDB)
  - Sequential writes: 348k ops/sec
  - Random reads: 5.5M ops/sec
  - Mixed 50/50: 642k ops/sec

7e421cb - feat: implement main DB interface with flush and compaction
  - Unified DB struct integrating all components
  - Public API: get(), put(), delete()
  - Automatic flush and compaction scheduling
  - 48 tests passing

ea3b5bd - feat: implement LSM compaction with merge iterator
  - LSM level structure (L0-L6)
  - Merge iterator (k-way merge, deduplication)
  - compact_sstables() function
  - 43 tests passing

[Previous commits...]
```

---

*Update this file every session - NO dated summaries, just current state*

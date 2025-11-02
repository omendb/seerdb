# STATUS - seerdb

**Last Updated**: November 1, 2025
**Current Phase**: Week 9 Complete - Learned Bloom Research
**Focus**: Research findings: learned blooms work for specific use cases only

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
  - Iterator support
  - Simple format (no blocks yet)
  - src/sstable/mod.rs: 425 lines

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

**Tests**: 63 passing (49 unit + 14 integration)
**Code**: 2,750+ lines (core engine + integration tests)
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

**Next**: Skip to Week 13 (KV Separation) - more applicable to seerdb

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

## Blockers

### None Currently
- Compaction system functional
- Ready for Week 8 (DB interface)
- All tests passing

---

## Next Session

**Week 8: Main DB Interface** (Integration week)

**Goal**: Create unified DB interface that ties everything together

**Tasks**:
1. DB struct (combines WAL, memtable, LSMTree)
2. Public API: get(), put(), delete(), scan()
3. Flush logic: Memtable → L0 SSTable
4. Compaction scheduling: Trigger on flush
5. File management: Delete old SSTables after compaction
6. Recovery: WAL replay on startup
7. Tests: End-to-end DB operations
8. Benchmark vs fjall (target: match 438k writes/sec)

**Why This Matters**:
- Currently have components but no unified interface
- Need to wire WAL → Memtable → SSTable → Compaction
- Integration is where bugs often hide
- Benchmark will validate design decisions

**Architecture Decision**:
- Synchronous compaction first (simple)
- Background thread later (Week 9+)

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

**Pending**:
- 📋 Background compaction (future)
- 📋 File cleanup after compaction (future)
- 📋 Learned bloom filters (Week 9+)
- 📋 Learned indexes (Week 10+)
- 📋 KV separation (Week 13+)

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
             ┌─────▼────▼──────┐
             │   SSTable (L0)  │  ← Disk storage + bloom filters
             └─────┬───────────┘
                   │ compact
             ┌─────▼───────────┐
             │   LSM Levels    │  ← L1-L6 (exponential sizing)
             │  (Compaction)   │
             └─────────────────┘

Functional: All components integrated, WAL recovery works
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

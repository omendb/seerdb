# STATUS - seerdb

**Last Updated**: November 1, 2025
**Current Phase**: Week 7 Complete - LSM Compaction Implemented
**Focus**: Core engine with compaction ready, Week 8: Main DB interface

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

**Tests**: 43 passing (39 unit + 4 integration)
**Code**: 2,180+ lines of core engine code
**Benchmarks**: Criterion-based SSTable performance tests

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

**Week 7 Just Completed**:
- ✅ LSM level structure
- ✅ Merge iterator (k-way merge)
- ✅ Compaction function
- ✅ Size/file-count triggers
- ✅ 43 tests passing

**Next: Week 8 - Main DB Interface**

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

**Tests**: 43 passing
- 39 unit tests (module-level)
- 4 integration tests (end-to-end)

---

## Architecture Progress

**Completed**:
- ✅ WAL for durability
- ✅ Memtable (skiplist)
- ✅ SSTable with bloom filters + binary search
- ✅ LSM compaction system
- ✅ Merge iterator

**Pending**:
- 📋 Main DB interface (Week 8)
- 📋 Flush coordination
- 📋 Background compaction
- 📋 File cleanup
- 📋 Learned bloom filters (Week 9+)
- 📋 Learned indexes (Week 10+)
- 📋 KV separation (Week 13+)

**Current Architecture**:
```
┌──────────────┐
│     WAL      │  ← Durability
└──────┬───────┘
       │
┌──────▼───────┐
│   Memtable   │  ← In-memory buffer
└──────┬───────┘
       │ flush
┌──────▼───────┐
│   SSTable    │  ← Disk storage (with bloom filters)
│   (L0-L6)    │
└──────┬───────┘
       │ compact
┌──────▼───────┐
│  Compaction  │  ← Merge SSTables
└──────────────┘

Missing: DB interface to coordinate all components
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
ea3b5bd - feat: implement LSM compaction with merge iterator
  - LSM level structure (L0-L6)
  - Merge iterator (k-way merge, deduplication)
  - compact_sstables() function
  - 43 tests passing

dd48aa1 - docs: Week 6 results and benchmark suite
  - SSTable benchmark suite
  - Binary search + bloom filter results
  - 32 tests passing

a4d2c8b - feat: enhance SSTable with binary search and bloom filters
  - Binary search: O(log n) lookups
  - Bloom filter: 19x speedup for negative lookups
  - 32 tests passing

[Previous commits...]
```

---

*Update this file every session - NO dated summaries, just current state*

# STATUS - seerdb

**Last Updated**: November 1, 2025
**Current Phase**: Week 6 Complete - SSTable Enhanced with Binary Search + Bloom Filters
**Focus**: Core engine functional, ready for Week 7 (LSM Compaction)

---

## Current State

### What We Have

**Research Complete** (7/9 papers, 78%):
- ✅ Phase 1 Foundational (3/3): Learned Indexes, ALEX, Learned Bloom Filters
- ✅ Phase 2 LSM Trees (3/3): WiscKey, Dostoevsky, PebblesDB
- ✅ Phase 3 Workload-Aware (1/1): Bourbon
- 📋 Phase 4 Modern Hardware (0/1): FASTER remaining (optional)

**Core Engine Complete** (Weeks 5-6):
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

**Tests**: 32 passing (28 unit + 4 integration)
**Code**: 1,320+ lines of core engine code
**Benchmarks**: Criterion-based SSTable performance tests

---

## Week 6 Results Summary

**Performance**:
- Point lookups (100k entries): 476k ops/sec (existing keys)
- Missing key lookups: **9.1M ops/sec** (19x faster with bloom filter)
- Full scans: 352k entries/sec (linear scaling)

**Key Achievement**: Bloom filter provides constant-time (~11 µs) rejection of missing keys regardless of SSTable size

**Comparison to RocksDB**:
- Existing keys: 2.2x slower (no block cache yet)
- Missing keys: **8.7x faster** (bloom filter advantage)

**Details**: See ai/WEEK6_RESULTS.md

---

## Active Work

**Week 6 Just Completed**:
- ✅ Binary search on SSTable index
- ✅ Bloom filter integration
- ✅ Benchmark suite (criterion)
- ✅ 32 tests passing

**Next: Week 7 - LSM Compaction**

---

## What Worked

### Week 5-6 Implementation
- **Rapid prototyping**: WAL + Memtable + SSTable in ~1 week
- **Test-driven**: 32 tests ensure correctness
- **Benchmarking validates**: Measured 19x bloom filter improvement
- **Research informs design**: Dostoevsky/WiscKey principles applied

### Integration Testing
- Crash recovery works (WAL replay)
- Flush to SSTable preserves data
- Delete operations handled correctly (tombstones)
- Concurrent writes safe (crossbeam-skiplist)

### Bloom Filter Integration
- Format extensibility: Added bloom filter without breaking compatibility
- Serialization efficient: Bit packing reduces size by 8x
- Performance validated: 19x speedup for negative lookups

---

## What Didn't Work

### Simplifications Made
- No block-based storage yet (simple key-value format)
- No compression yet (LZ4 deferred to later)
- No block cache (LRU deferred to later)
- Rationale: Get core LSM working first, optimize later

### Format Trade-offs
- Current format reads full key+value on miss (inefficient)
- Block-based format would read only key (better)
- Acceptable for now - Week 7 may revisit

---

## Blockers

### None Currently
- Core engine is functional
- Clear path to Week 7 (compaction)
- All tests passing

---

## Next Session

**Week 7: LSM Compaction** (Critical for performance)

**Goal**: Implement leveled compaction to keep read amplification bounded

**Tasks**:
1. Compaction strategy (leveled or lazy leveling)
2. Level management (size ratios, triggers)
3. SSTable merging (sorted merge of multiple SSTables)
4. Tombstone handling (remove during compaction)
5. Background thread for compaction
6. Tests: compaction correctness, level invariants

**Why This Matters**:
- Without compaction: Reads become O(N * log M) where N = # SSTables
- With compaction: Reads bounded to O(levels * log M)
- Example: 100M entries without compaction = 100 SSTables
- Example: 100M entries with compaction = 4-5 levels (logarithmic)

**Architecture Decisions Needed**:
- Leveled vs Lazy Leveling (Dostoevsky recommends lazy)
- Level size ratios (T=10 standard)
- Compaction triggers (level size thresholds)

**Key Reference**: Dostoevsky paper (lazy leveling for mixed workloads)

---

## Key Metrics

**Lines of Code**:
- WAL: 411 lines
- Memtable: 234 lines
- SSTable: 425 lines
- Bloom: 252 lines (traditional)
- Integration tests: 194 lines
- Benchmarks: 90 lines
- **Total: ~1,600 lines**

**Performance** (SSTable, 100k entries):
- Existing key lookup: 2.1 µs
- Missing key lookup: 109 ns (19x faster)
- Full scan: 28.4 ms (10k entries)

**Tests**: 32 passing
- 28 unit tests (module-level)
- 4 integration tests (end-to-end)

---

## Architecture Progress

**Completed**:
- ✅ WAL for durability
- ✅ Memtable (skiplist)
- ✅ SSTable with bloom filters
- ✅ Binary search on index

**Pending**:
- 📋 LSM compaction (Week 7)
- 📋 Multi-level structure
- 📋 Background compaction thread
- 📋 Learned bloom filters (Week 9+)
- 📋 Learned indexes (Week 10+)
- 📋 KV separation (Week 13+)

**Format**:
```
Current:
[entries...][index][bloom_len][bloom_filter][footer]

Future (with blocks):
[data_blocks...][index_blocks...][bloom][footer]
```

---

## Research Insights Applied

1. **Dostoevsky (lazy leveling)**:
   - Will implement in Week 7
   - Upper levels: tiered (faster writes)
   - Largest level: leveled (better reads)

2. **WiscKey (KV separation)**:
   - Deferred to Week 13
   - Threshold: 4KB (perfect for omen vectors)

3. **Bourbon (adaptive learning)**:
   - Will use CBA for learned components
   - Only train on long-lived SSTables

4. **Learned bloom filters**:
   - Week 9 implementation
   - Use traditional below 10k keys
   - Use learned above 10k (73% space savings)

---

## Competitive Analysis

**fjall Baseline** (from baseline_benchmark):
- Writes: 438k ops/sec
- Mixed: 576k ops/sec
- **Target**: Match or beat with core engine, exceed with learned components

**seerdb Progress**:
- SSTable reads: 476k ops/sec (existing), 9.1M ops/sec (missing)
- Full LSM performance TBD (needs compaction)

**Differentiation** (vs fjall):
- Binary search: ✅ Implemented (O(log n) vs fjall's O(n))
- Bloom filters: ✅ Implemented (fjall has basic bloom)
- Learned bloom: 📋 Week 9 (fjall has ZERO learned components)
- Learned index: 📋 Week 10 (fjall uses binary search)
- SIMD: 📋 Week 14 (fjall has ZERO SIMD)

**Details**: See ai/research/FJALL_ANALYSIS.md, ai/COMPETITIVE_ADVANTAGES.md

---

## Recent Commits

```
a4d2c8b - feat: enhance SSTable with binary search and bloom filters
  - Binary search: O(log n) lookups
  - Bloom filter: 19x speedup for negative lookups
  - 32 tests passing

[Previous commits...]
598244a - chore: initial project structure
```

---

*Update this file every session - NO dated summaries, just current state*

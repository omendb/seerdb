# seerdb - Research-Grade Storage Engine

**Repository**: seerdb (Storage Engine with Learned Data Structures)
**Last Updated**: November 14, 2025
**License**: Elastic License 2.0 (source-available)
**Status**: Testing complete (0.0.0 pre-alpha) - working towards 0.0.1 (4-5 weeks)

---

## Product Overview

**seerdb**: Modern embedded storage engine implementing 2018-2024 research

**What It Is**:
- LSM-tree based storage engine (like RocksDB)
- Learned data structures replace traditional components
- Workload-aware optimization
- Optimized for vectors, time series, and high-throughput workloads
- Rust-native with modern hardware optimizations

**Positioning**: "RocksDB but with 2020s research - 4.82x better write amplification (validated)"

**Why This Matters**: Storage engines are 10+ years old. Decade of research (learned indexes, workload-aware LSM, key-value separation) not integrated into production systems. seerdb bridges this gap.

**Market**: Foundation for database builders, embedded systems, high-performance applications

---

## Quick Start for AI Agents

**→ First time?** Load these in order:
1. This file (CLAUDE.md) - Project overview
2. `ai/CURRENT_STATE.md` - TL;DR current status (start here!)
3. `ai/PRODUCTION_READINESS.md` - Roadmap to 0.0.1 (8 weeks)
4. `ai/BUGS_AND_EDGE_CASES.md` - All known bugs (critical to minor)
5. `ai/DECISIONS.md` - Design decisions with rationale

**→ Continuing work?** Check `ai/CURRENT_STATE.md` first, then `ai/TODO.md`

**→ Refactoring context?** See `CONTEXT.md` for recent code improvements (Nov 14, 2025)

**→ Full documentation guide**: See `ai/README.md` for all available docs

---

## Recent Work: Code Quality Improvements (Nov 14, 2025) ✅

**Database Refactoring Complete:**
- ✅ Extracted background workers into dedicated module (477 lines)
- ✅ Extracted utility helpers into dedicated module (143 lines)
- ✅ Reduced `src/db.rs` from 3,654 to 3,141 lines (-14.0%)
- ✅ Simplified `DB::open()` initialization by 47%
- ✅ All 146 tests passing, zero functional changes
- ✅ Better code organization and maintainability

**Branch:** `claude/seerdb-extract-background-workers-01H58mQr9RbAq7QcRSUGgriE` (ready to merge)

**Documentation:**
- See `CONTEXT.md` for full refactoring summary
- See `ai/REFACTORING_SUMMARY.md` for detailed technical analysis

---

## Current Phase: Testing Complete → Documentation (0.0.1 Preparation)

**Status**: ✅ **Testing Phase Complete!** - 81.54% coverage (exceeded 80% goal), ASAN clean, 271 tests passing

**Latest Performance** (jemalloc + ArcSwap + SIMD - Nov 8, 2025):
- **Writes**: 878K ops/sec (2.47x RocksDB, 2.06x fjall) 🏆
- **Reads**: 2,207K ops/sec (2.07x RocksDB, 1.90x fjall) 🏆  
- **Mixed**: 718K ops/sec (1.79x RocksDB, 0.86x fjall)
- **Scans**: 19.6K scans/sec (0.99x RocksDB, 0.98x fjall)
- **Write amp**: 1.01x (4.82x better than traditional LSM) 🏆

**Optimizations Complete**:
- ✅ LZ4 block compression (+34.7% writes)
- ✅ jemalloc allocator (+17-21% all workloads)
- ✅ ArcSwap lock-free structures (+1-4%)
- ✅ SIMD key comparison (+3-4% reads)
- ✅ ALEX learned index (+55% reads)
- ✅ Partitioned memtables (16 partitions)
- ✅ Lock-free WAL
- ✅ Decompressed block cache
- ✅ foldhash (2x faster hashing)
- ✅ varint-rs (space-efficient encoding)

**Critical Issues Status**: ✅ **ALL FIXED!**
1. ✅ Block cache unbounded (FIXED - quick_cache LRU, 10K blocks, ~40MB limit)
2. ✅ Batch API non-atomic (FIXED - single WAL batch record, atomic recovery)
3. ✅ No checksums (FIXED - SSTable footer checksum validated on read)
4. ✅ No magic numbers (FIXED - WAL/VLog have magic numbers + version)
5. ✅ Iterator invalidation (FIXED - memtables collected before SSTables)
6. ⏸️ VLog GC race (DEFERRED - GC not implemented yet, will be done correctly in 0.0.2+)
7. ✅ Compaction can delete live keys (FIXED - delayed deletion queue)
8. ✅ WAL recovery race (FIXED - barrier synchronization + file cursor seek)
9. ✅ Tombstone handling in SSTables (FIXED - SSTable.contains() distinguishes tombstone from miss)

**Testing Phase Complete (Nov 10, 2025)**:
- ✅ All critical bugs fixed!
- ✅ All tests passing (271 tests, 0 failures)
- ✅ **81.54% test coverage achieved** (exceeded 80% goal)
- ✅ **Memory safety validated** (ASAN clean)
- ✅ **Thread safety validated** (50+ concurrent tests)
- ✅ Production hardening complete

**Remaining Work for 0.0.1**:
- ❌ Documentation (API guide, architecture, examples)
- ❌ Long-running stability tests (optional)
- ❌ Final validation & release prep

**Next Focus**: Documentation or declare testing complete

---

## Success Metrics

### Current Performance ✅
- ✅ All tests passing (100% pass rate)
- ✅ Write amp: 1.01x (4.82x better than traditional LSM)
- ✅ Writes: 2.47x RocksDB (best-in-class) 🏆
- ✅ Reads: 2.07x RocksDB (best-in-class) 🏆
- ✅ Mixed: 1.79x RocksDB 🏆
- ⚠️ Mixed: 0.86x fjall (14% gap - investigating)

### Quality Status
- ✅ Test coverage: 81.54% (exceeded 80% goal for 0.0.1)
- ✅ Crash recovery: All tests passing
- ✅ Memory safety: ASAN clean, Rust + minimal unsafe
- ✅ Thread safety: 50+ concurrent tests passing
- ✅ Data loss prevention: All critical bugs fixed
- ✅ Compaction safety: Tombstone + deletion queue fixes prevent data loss
- ✅ Performance claims: Documented with benchmarks
- ⚠️ Production ready: 4-5 weeks (documentation + final validation)

### Isolation Level
- **Current**: Read Committed (per-operation snapshot consistency)
- **Future (0.0.2+)**: Snapshot Isolation (multi-operation MVCC)
- **Rationale**: Vector databases (Milvus, Qdrant, Weaviate) use eventual consistency for ANN search. Read Committed is sufficient for vector database workloads. MVCC deferred to 0.0.2+ based on user feedback. See: ai/research/LSM_MVCC_CONCURRENCY_RESEARCH.md

---

## Workload Optimization

### Vector Database Workloads

**Characteristics**:
- Large values (embeddings: 512-4096 bytes)
- Append-heavy (new documents)
- Range scans (vector search results)
- Hot/cold data (recent docs hot)

**seerdb Optimizations**:
- Key-value separation (large embeddings separate - vLog)
- Learned index (ALEX - predict document ID patterns)
- LZ4 compression (embeddings highly compressible)
- Workload-aware compaction

### Time Series Workloads

**Characteristics**:
- Sorted by timestamp
- Range queries (time windows)
- Compression-friendly (similar values)
- Long retention (old data archived)

**seerdb Optimizations**:
- Time-aware compaction (merge by time ranges)
- Aggressive compression (delta encoding + LZ4)
- Hot/cold separation (recent data hot)
- Efficient range scans

### Queue Workloads

**Characteristics**:
- Small values (job metadata: <1KB)
- High write throughput (enqueue)
- FIFO access pattern
- Short retention (jobs processed quickly)

**seerdb Optimizations**:
- Partitioned memtables (16 partitions)
- Lock-free WAL
- Fast memtable flush (reduce queue latency)
- Tiered compaction (optimize for sequential writes)

---

## Competitive Analysis

### RocksDB (Baseline)

**Pros**:
- Battle-tested, production-proven
- Rich feature set
- Good documentation

**Cons**:
- C++ (harder to integrate with Rust)
- Generic design (not workload-optimized)
- Write amplification issues
- No learned components

**Our Advantage**: 2020s research, Rust-native, 1.8x-2.5x faster

### fjall (Rust, 2023)

**Pros**:
- Modern Rust LSM
- Clean design
- Good mixed workload performance

**Cons**:
- No learned components
- No workload-aware optimizations

**Our Advantage**: Learned components (ALEX), better writes/reads, investigating mixed gap

### sled (Rust)

**Pros**:
- Rust-native
- Simpler than RocksDB
- Lock-free B+ tree

**Cons**:
- B+ tree (not LSM) - worse for writes
- No learned components
- Less mature

**Our Advantage**: LSM better for writes, learned components, 13x faster writes

---

## Key Papers Implemented

1. ✅ "ALEX: An Updatable Adaptive Learned Index" (Ding et al., MIT/Columbia 2020)
   - Implemented: O(log error) lower_bound, +55% read performance

2. ✅ "WiscKey: Separating Keys from Values" (Lu et al., Wisconsin 2016)
   - Implemented: vLog for large values, 4.82x better write amp

3. ✅ "Dostoevsky: Better Space-Time Trade-Offs" (Dayan et al., Harvard 2018)
   - Implemented: Optimal level ratios for workload

4. 📚 "FASTER: A Concurrent Key-Value Store" (Microsoft 2018)
   - Inspired: Lock-free structures (ArcSwap, lock-free WAL)

5. 📚 LZ4 compression (Yann Collet)
   - Implemented: Block compression, +34.7% writes

---

## 📚 Research Foundation (November 14, 2025)

**SOTA Research Complete**:

**Phase 1: LSM Engines** (`ai/research/lsm_engines_sota.md`)
- 12 papers on LSM-tree internals, buffer management, compaction
- WiscKey, Monkey, Dostoevsky, SILK, etc.
- Foundation for seerdb's LSM optimizations

**Phase 4: General Storage** (`ai/research/general_storage_engine_sota.md`)
- LeanStore pointer swizzling (40-60% speedup)
- Bw-tree lock-free design (100% improvement)
- Mini-page optimization for in-memory workloads
- Future path for seerdb buffer manager improvements

**Architecture Spec** (`ai/design/seerdb_core_architecture.md`)
- Complete prescriptive architecture for seerdb-core
- Public MIT-licensed LSM engine specification
- Six-layer architecture: API → Buffer Pool → WAL → MemTable → SSTable → Compaction
- Performance targets: 763K ops/sec baseline, LeanStore integration path

**Code Reuse Strategy**:
- seerdb: Foundation for vector databases and embedded applications
- Same LSM foundation, different frontends
- Fix bugs once, all products benefit

---

## Development Principles

**Research-Driven**:
- Every design decision backed by paper or benchmark
- Document trade-offs clearly
- Validate research claims with experiments

**Iteration Speed**:
- Prototype ideas quickly
- Benchmark early and often
- Ship functional core fast, optimize based on profiling

**Code Quality**:
- Comprehensive tests (unit + integration + stress)
- Performance benchmarks for critical paths
- Clear documentation and examples
- Zero unsafe code where possible

---

## Repository Structure

```
seerdb/
├── AGENTS.md              # Primary AI agent entry point
├── CLAUDE.md → AGENTS.md  # Symlink for Claude Code compatibility
├── README.md              # Public documentation
├── LICENSE                # Elastic License 2.0
├── Cargo.toml             # Rust package manifest
├── src/
│   ├── lib.rs             # Public API
│   ├── wal/               # Write-ahead log
│   ├── memtable/          # In-memory buffer (partitioned skiplist)
│   ├── sstable/           # SSTable format
│   │   ├── learned_bloom/ # Learned bloom filters (planned)
│   │   └── alex/          # ALEX learned index (implemented)
│   ├── compaction/        # Compaction strategies
│   ├── vlog/              # Value log (WiscKey)
│   ├── cache/             # Block cache (quick_cache)
│   └── simd/              # SIMD optimizations
├── examples/              # Usage examples + benchmarks
├── benches/               # Performance benchmarks
├── tests/                 # Integration tests
└── ai/                    # AI session context (continuity across sessions)
    ├── README.md                  # Documentation guide
    ├── CURRENT_STATE.md           # TL;DR current status (read FIRST)
    ├── PRODUCTION_READINESS.md    # Roadmap to 0.0.1
    ├── BUGS_AND_EDGE_CASES.md     # All known bugs
    ├── STATUS.md                  # Current state + recent learnings
    ├── TODO.md                    # Active tasks only
    ├── DECISIONS.md               # Design decisions index
    ├── decisions/                 # Detailed decisions by topic
    │   ├── architecture.md        # Core architectural decisions
    │   ├── performance.md         # Performance optimizations
    │   ├── storage.md             # Storage format decisions
    │   ├── compaction.md          # Compaction strategies
    │   ├── concurrency.md         # Thread safety & isolation
    │   └── superseded-2025-11.md  # Historical/completed decisions
    ├── research/                  # Detailed research (loaded on demand)
    └── design/                    # Design specifications
```

### AI Context Organization

**Purpose**: AI uses `ai/` to maintain continuity between sessions

**Session files** (ai/ root - read every session, <500 lines each):
- `CURRENT_STATE.md` — Current status, blockers (read FIRST)
- `STATUS.md` — Performance metrics, recent learnings
- `TODO.md` — Active tasks only (no completed tasks)
- `DECISIONS.md` — Design decisions index
- `PRODUCTION_READINESS.md` — Roadmap to 0.0.1
- `BUGS_AND_EDGE_CASES.md` — Known bugs

**Reference files** (subdirectories - loaded only when needed):
- `decisions/` — Detailed decisions by topic (architecture, performance, storage, etc.)
- `research/` — Detailed research (>200 lines per topic)
- `design/` — Design specifications

**Token Efficiency**: Session files total ~2,500 tokens (from 35,000+). Reference files loaded on demand.

**Maintenance**: Session files kept current and focused. Completed items deleted (git preserves history).

---

## Roadmap to 0.0.1 (4-5 Weeks Remaining)

### Phase 1: Critical Bugs (Data Safety) ✅ **COMPLETE**
- ✅ Block cache (quick_cache with size limits)
- ✅ Batch API atomicity (single WAL record)
- ✅ Checksums (CRC32 for all data blocks)
- ✅ Magic numbers + version detection
- ✅ Iterator invalidation fix

### Phase 2: Production Hardening ✅ **COMPLETE**
- ✅ Memory budget enforcement
- ✅ Disk space checks
- ✅ File descriptor limits
- ✅ SSTable fsync
- ✅ Background panic handling
- ✅ Compaction live key deletion fix
- ⏸️ VLog GC race fix (deferred - not implemented yet)

### Phase 3: Comprehensive Testing ✅ **COMPLETE**
- ✅ ALEX tests (20 tests, 462 LOC)
- ✅ VLog tests (24 tests, 631 LOC)
- ✅ Coverage measurement: **81.54%** (exceeded 80% goal)
- ✅ ASAN: ALL PASSED (no memory issues)
- ✅ Thread safety: 50+ concurrent tests
- ✅ 271 tests passing (0 failures)

### Phase 4: Documentation 📚 (Next Priority)
- Complete API documentation
- Architecture guide
- Performance tuning guide
- Examples (5+)
- **Status**: Not started

### Phase 5: Buffer & Release 🚀
- Full validation
- Long-running stability tests (optional)
- Release notes
- Version tagging (0.0.1)
- **Status**: Not started

### Deferred to 0.0.2+ (Post-Release)
- rkyv zero-copy (only +3% benefit, high complexity)
- Multi-tier caching (needs production workload data)
- Close fjall mixed gap (already 1.79x faster than RocksDB)
- Advanced learned components
- **Rationale**: Correctness > optimization

---

*Last Updated: November 10, 2025 - Testing phase complete*

**Product**: seerdb - Research-grade storage engine
**Status**: Testing complete (0.0.0 pre-alpha) - working towards 0.0.1 documentation
**Performance**: 878K writes/sec, 2.2M reads/sec (2.5x RocksDB in benchmarks) 🏆
**Quality**: 81.54% coverage (exceeded 80% goal), ASAN clean, 271 tests passing (0 failures)
**Critical Issues**: ✅ ALL FIXED (7/7 complete, 1 deferred to 0.0.2+)
**Recent Achievement**: Testing phase complete - coverage goal exceeded, memory/thread safety validated
**Timeline**: 4-5 weeks to 0.0.1 (documentation + final validation)
**Next**: Documentation or declare testing complete

# seerdb - Research-Grade Storage Engine

**Repository**: seerdb (Storage Engine with Learned Data Structures)
**Last Updated**: November 16, 2025
**License**: Apache-2.0
**Status**: PRE-ALPHA - MISSING STANDARD API FEATURES (range iterators, snapshots)

---

## Product Overview

**seerdb**: Modern embedded storage engine implementing 2018-2024 research

**What It Is**:
- LSM-tree based storage engine (like RocksDB)
- Learned data structures replace traditional components
- Workload-aware optimization
- Optimized for large values, time series, and high-throughput workloads
- Rust-native with modern hardware optimizations

**Positioning**: "RocksDB but with 2020s research - 4.82x better write amplification (validated)"

**Why This Matters**: Storage engines are 10+ years old. Decade of research (learned indexes, workload-aware LSM, key-value separation) not integrated into production systems. seerdb bridges this gap.

**Market**: Foundation for database builders, embedded systems, high-performance applications

---

## Quick Start for AI Agents

**→ First time?** Load these in order:
1. This file (CLAUDE.md) - Project overview
2. `ai/STATUS.md` - Current state (read FIRST - has honest assessment)
3. `ai/design/NEXT_API_PRIORITIES.md` - Missing features roadmap
4. `ai/design/API_COMPARISON_TABLE.md` - Shows gaps vs competitors
5. `CONTEXT.md` - Session context

**→ Continuing work?** Check `ai/STATUS.md` first, then `ai/TODO.md`

**→ API research?** See `ai/research/LSM_API_RESEARCH_SUMMARY.md` for competitor analysis

**→ Full documentation guide**: See `ai/README.md` for all available docs

---

## ⚠️ CRITICAL: API Completeness Issues (Nov 16, 2025)

**REALITY CHECK**: seerdb is missing fundamental features that all competitors have.

### What We Have (Works Well)
```rust
// ✅ Point operations only
db.get(key)           // Point lookup
db.put(key, value)    // Write
db.delete(key)        // Delete
db.batch()            // Atomic batch writes
db.flush()            // Sync to disk
db.get_stats()        // Observability
db.check_health()     // Health checks
```

### What's Missing (CRITICAL)
```rust
// ❌ ALL COMPETITORS HAVE THESE
db.range(start..end)  // NO RANGE QUERIES - blocks 70% of use cases
db.iter()             // NO ITERATION AT ALL
db.prefix(prefix)     // NO PREFIX SCANS
db.snapshot()         // NO CONSISTENT MULTI-READ VIEWS
db.transaction()      // NO MVCC/TRANSACTIONS
```

**Impact**: Without range iterators, seerdb blocks:
- Time series queries (by time range)
- Analytics (scanning data)
- Pagination (iterating results)
- Queue patterns (ordered dequeue)
- Secondary indexes

**See**: `ai/design/API_COMPARISON_TABLE.md` for full gap analysis

---

## Current Phase: API Completeness (PRE-ALPHA)

**Status**: ❌ **INCOMPLETE** - Missing standard features, NOT ready for release

**What's Good**:
- ✅ Performance: 2.47x RocksDB writes, 2.07x reads
- ✅ Tests: 271 passing, 81.54% coverage, ASAN clean
- ✅ Critical bugs fixed (batch atomicity, checksums, etc.)

**What's Missing**:
- ❌ **Range iterators** - CRITICAL, blocks most use cases
- ❌ **Snapshots** - No consistent multi-read views
- ❌ **Iteration** - Can't scan or paginate
- ❌ **Cloud storage backend** - No S3/GCS support

**Performance** (valid for point operations only):
- Writes: 878K ops/sec (2.47x RocksDB) 🏆
- Reads: 2,207K ops/sec (2.07x RocksDB) 🏆
- Write amp: 1.01x (4.82x better than traditional LSM) 🏆

**Optimizations Applied** (all for point operations):
- LZ4 block compression, jemalloc allocator
- ALEX learned index, ArcSwap lock-free structures
- Partitioned memtables (16), lock-free WAL
- foldhash, varint-rs encoding

**Bug Fixes Complete**:
- ✅ Block cache bounded (quick_cache LRU)
- ✅ Batch API atomic (single WAL record)
- ✅ Checksums (CRC32 validated on read)
- ✅ Magic numbers + version detection
- ✅ Compaction safety (delayed deletion queue)

**Next Priority**: Implement missing API features before ANY release

---

## Recent Work: API Audit (Nov 16, 2025)

**Critical Discovery**: Competitor analysis revealed major API gaps
- Created `ai/design/API_COMPARISON_TABLE.md` - Gap analysis
- Created `ai/design/NEXT_API_PRIORITIES.md` - Implementation roadmap
- Created `ai/research/LSM_API_RESEARCH_SUMMARY.md` - Full research
- Updated `ai/STATUS.md` - Honest assessment (PRE-ALPHA)
- Fixed CI failures (SIMD feature gates, clippy rules)

**Key Learning**: Performance benchmarks ≠ feature completeness. We tested only what exists, not what should exist.

---

## Previous Work: Code Refactoring (Nov 14, 2025) ✅

**Database Refactoring Complete:**
- ✅ Extracted background workers into dedicated module (477 lines)
- ✅ Extracted utility helpers into dedicated module (143 lines)
- ✅ Reduced `src/db.rs` from 3,654 to 3,141 lines (-14.0%)
- ✅ Simplified `DB::open()` initialization by 47%
- ✅ All 146 tests passing, zero functional changes

---

## Success Metrics

### Current Performance ✅ (Point Operations Only)
- ✅ All tests passing (271 tests, 0 failures)
- ✅ Write amp: 1.01x (4.82x better than traditional LSM)
- ✅ Writes: 2.47x RocksDB (point operations) 🏆
- ✅ Reads: 2.07x RocksDB (point operations) 🏆
- ❌ Range queries: NOT IMPLEMENTED
- ❌ Iteration: NOT IMPLEMENTED

### Quality Status
- ✅ Test coverage: 81.54% (for existing features)
- ✅ Memory safety: ASAN clean
- ✅ Thread safety: 50+ concurrent tests passing
- ✅ Data integrity: Checksums, batch atomicity, compaction safety
- ❌ **API completeness: MISSING fundamental features (range iterators, snapshots)**
- ❌ Production ready: NO - missing standard functionality

### Feature Completeness
- ✅ Point operations (get/put/delete/batch)
- ✅ Observability (stats, health checks)
- ❌ **Range queries** - CRITICAL MISSING
- ❌ **Iteration** - CRITICAL MISSING
- ❌ **Snapshots** - HIGH PRIORITY MISSING
- ❌ **Transactions/MVCC** - MEDIUM PRIORITY MISSING
- ❌ **Cloud storage** - HIGH PRIORITY MISSING

---

## Workload Optimization

### Large Value Workloads

**Characteristics**:
- Large values (blobs/documents: 512-4096 bytes)
- Append-heavy (new entries)
- Range scans (sequential access)
- Hot/cold data (recent entries hot)

**seerdb Optimizations**:
- Key-value separation (large values separate - vLog)
- Learned index (ALEX - predict ID patterns)
- LZ4 compression (large values highly compressible)
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
- seerdb: Foundation for databases and embedded applications
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

## Roadmap to 0.0.1 (Timeline: TBD - BLOCKED on feature audit)

### Phase 1: Bug Fixes ✅ **COMPLETE**
- ✅ Block cache bounded
- ✅ Batch API atomicity
- ✅ Checksums and magic numbers
- ✅ Compaction safety

### Phase 2: Testing Infrastructure ✅ **COMPLETE**
- ✅ 271 tests, 81.54% coverage
- ✅ ASAN clean, thread safety validated

### Phase 3: API Completeness ❌ **CRITICAL - NOT STARTED**
**Timeline**: TBD (need full audit first)

1. **Range Iterators** - CRITICAL (blocks 70% of use cases)
   - `db.range(start..end)` - range queries
   - `db.iter()` - full iteration
   - `db.prefix(prefix)` - prefix scans
   - `db.iter_rev()` - reverse iteration

2. **Snapshots** - HIGH PRIORITY
   - `db.snapshot()` - point-in-time views
   - Consistent multi-read operations

3. **Cloud Storage** - HIGH PRIORITY
   - S3/GCS backend via object_store crate
   - Hybrid: local memtable + cloud SSTables

4. **Transactions** - MEDIUM PRIORITY
   - MVCC for multi-key atomicity
   - Full ACID guarantees

### Phase 4: Stability & Fuzzing 📊 (After API Complete)
- 24+ hour fuzzing campaigns
- Long-running soak tests
- Chaos/fault injection

### Phase 5: Documentation & Release 🚀
- Complete API docs (minimal)
- Examples
- Version tagging (0.0.1)

**No release until API features complete and stable.**

---

*Last Updated: November 16, 2025 - API audit revealed critical gaps*

**Product**: seerdb - Research-grade storage engine
**Status**: PRE-ALPHA - MISSING STANDARD API FEATURES
**Performance**: 878K writes/sec, 2.2M reads/sec (point operations only) 🏆
**Quality**: 81.54% coverage, ASAN clean, 271 tests passing
**Critical Gap**: ❌ NO RANGE ITERATORS (blocks 70% of use cases)
**Missing Features**: Range queries, iteration, prefix scans, snapshots, transactions
**Timeline**: TBD - blocked on feature audit and implementation
**Next**: Full feature audit, then implement missing API (no release until complete)

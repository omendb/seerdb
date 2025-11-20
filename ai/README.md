# ai/ Directory - Development Documentation

**Purpose**: Session context and development documentation for seerdb
**Audience**: AI agents and developers working on seerdb
**Last Updated**: November 17, 2025

---

## Quick Start

**New to the project?** Read in this order:
1. `STATUS.md` - Current state (Production-ready ✅)
2. `TODO.md` - Completed work + optional optimizations
3. `REMAINING_WORK.md` - **NEW**: Detailed breakdown of optional work
4. `PLAN.md` - Strategic roadmap for graph/vector workloads

---

## Directory Structure

```
ai/
├── STATUS.md                      # Current state, metrics, recent progress
├── TODO.md                        # Active tasks and priorities
├── PROFILING_RESULTS.md           # Latest profiling work (Nov 17, 2025)
├── OPTIMIZATION_PREFIX_ITERATION.md # Current optimization work
├── PRODUCTION_READINESS.md        # Production roadmap
├── BUGS_AND_EDGE_CASES.md         # Active bug tracking
├── README.md                      # This file
├── DECISIONS.md                   # Decision index
├── RESEARCH.md                    # Research index
│
├── bugs/                          # Historical bug tracking
│   ├── BUG_10_BACKGROUND_FLUSH_DATA_LOSS.md
│   └── BUG_11_ALEX_KEY_COLLISION.md
│
├── decisions/                     # Detailed design decisions
│   ├── architecture.md
│   ├── performance.md
│   ├── storage.md
│   ├── compaction.md
│   ├── concurrency.md
│   └── superseded-2025-11.md
│
├── design/                        # Design specifications
│   ├── seerdb_core_architecture.md
│   ├── API_COMPARISON_TABLE.md
│   ├── NEXT_API_PRIORITIES.md
│   ├── ARCHITECTURE.md
│   ├── API_DESIGN.md
│   ├── API_REVIEW.md
│   ├── TIERED_STORAGE_ROADMAP.md
│   └── ...
│
├── performance/                   # Performance analysis archive
│   ├── STORAGE_BENCHMARK_RESULTS.md
│   ├── STORAGE_OPTIMIZATION_ANALYSIS.md
│   ├── HOT_PATH_ANALYSIS.md
│   └── BLOCK_CACHE_OPTIMIZATION.md
│
├── research/                      # Research findings
│   ├── INDEX.md
│   ├── LSM_API_RESEARCH_SUMMARY.md
│   ├── lsm_engines_sota.md
│   ├── general_storage_engine_sota.md
│   └── ...
│
├── summaries/                     # Historical summaries
│   └── REFACTORING_SUMMARY.md
│
└── testing/                       # Test results archive
    ├── STRESS_TEST_RESULTS.md
    ├── SOAK_TESTING.md
    ├── FUZZING.md
    ├── COVERAGE_REPORT.md
    └── ...
```

---

## Session Files (Read Every Session)

**Core files** (read every session):

1. **`STATUS.md`** - Current state, metrics, recent progress
   - Last updated: November 18, 2025
   - Contains: Phase 4 findings, performance reality check, graph analysis
   - Key insight: seerdb is ALREADY fast for graph workloads with `SyncPolicy::None`

2. **`TODO.md`** - Active optimization priorities and timeline
   - Last updated: November 18, 2025
   - Contains: Critical optimizations (group commit, WAL pipelining)
   - Focus: General-purpose improvements

3. **`PROFILING_RESULTS.md`** - Phase 1 CPU profiling (flamegraph)
   - Date: November 17, 2025
   - Contains: Flamegraph results, CPU hotspots, cache validation
   - Read: When working on performance

3a. **`ALLOCATION_PROFILING.md`** - Phase 2 memory profiling (dhat)
   - Date: November 17, 2025
   - Contains: Heap allocation patterns, peak memory (30-32 MB), optimization opportunities
   - Read: When optimizing memory usage

3b. **`LOCK_CONTENTION_ANALYSIS.md`** - Phase 3 concurrency profiling
   - Date: November 17, 2025
   - Contains: Lock contention analysis, WAL bottleneck, parallel efficiency metrics
   - Read: When optimizing concurrent performance

3c. **`REAL_WORKLOAD_COMPARISONS.md`** - Phase 4 realistic workload comparisons
   - Date: November 18, 2025
   - Contains: seerdb vs RocksDB vs fjall on realistic workloads, performance discrepancy analysis
   - Read: When evaluating production readiness or performance claims

3d. **`GRAPH_REQUIREMENTS_ANALYSIS.md`** - Graph durability analysis
   - Date: November 18, 2025
   - Contains: Do we need durability? SOTA for vector databases, configuration recommendations
   - Read: When configuring seerdb for graph or vector database workloads

4. **`OPTIMIZATION_PREFIX_ITERATION.md`** - Prefix iteration optimizations (completed)
   - Date: November 17, 2025
   - Contains: Key-only iteration (5.68x), read-ahead prefetching, batch API
   - Read: When working on range iteration optimizations

5. **`GRAPH_PERFORMANCE_IMPACT.md`** - Graph integration results
   - Date: November 17, 2025
   - Contains: seerdb integration into vector DB, cache hit rate analysis
   - Read: When evaluating graph-specific performance

---

## Index Files (Reference On Demand)

7. **`DECISIONS.md`** - Design decisions index
   - Points to: `decisions/` subdirectory
   - Contains: Architecture, performance, storage, compaction, concurrency decisions

---

## Reference Subdirectories (Loaded On Demand)

### `bugs/` - Historical Bug Tracking
- Fixed bugs with detailed analysis (Bug #10, #11)
- Archived: `BUGS_AND_EDGE_CASES_ARCHIVE.md` (all critical bugs fixed)

### `decisions/` - Detailed Design Decisions
- Architecture, performance, storage, compaction, concurrency
- `superseded-2025-11.md` - Historical decisions from research phase

### `design/` - Design Specifications
- Architecture specs, API designs, format specifications
- Roadmaps for cloud storage, tiered storage, research

### `performance/` - Performance Analysis Archive
- Historical benchmarks, optimization analyses
- Block cache optimization, hot path analysis

### `research/` - Research Findings
- Paper summaries (LSM engines, general storage, prefix iteration)
- Competitive analysis (RocksDB, fjall, sled)
- SOTA research (2024-2025)

### `summaries/` - Historical Summaries
- Refactoring summary, production readiness archive
- Milestone reports

### `testing/` - Test Results Archive
- Stress tests, soak tests, fuzzing results
- Coverage reports, sanitizer results, crash recovery tests

---

## Workflow

### Starting a Session
1. Read `STATUS.md` - Understand current state
2. Read `TODO.md` - Check active tasks
3. Check `BUGS_AND_EDGE_CASES.md` - Known issues

### During Work
- Update `TODO.md` - Mark tasks in_progress/completed
- Consult `decisions/` or `research/` as needed
- Add to `BUGS_AND_EDGE_CASES.md` if bugs found

### Ending a Session
- Update `STATUS.md` - Document progress
- Update `TODO.md` - Mark completed, add new tasks
- Commit changes to git

---

## Key Principles

### Documentation Strategy
- **Session files** (ai/ root): Current, active, <500 lines
- **Reference files** (subdirs): Detailed, loaded on demand
- **Token efficiency**: ~2,500 tokens for session context (down from 35,000+)

### Maintenance
- Delete outdated files immediately (git preserves history)
- Archive completed work to subdirectories
- Keep session files focused and current
- Update "Last Updated" on every edit

### Organization
- Active work → ai/ root
- Historical analysis → subdirectories
- Completed work → archive
- Research → `research/`

---

## Current Status (November 20, 2025)

**Production Readiness**: ✅ **READY**

**Stability** (Nov 20, 2025):
- All 182 tests passing (0 ignored)
- Zero data loss bugs (WAL race condition fixed)
- Zero panics on error paths (BufferPool errors handled gracefully)
- Data durability guaranteed (WAL fsync on shutdown)

**Performance**:
- Writes: 878K ops/sec (Mac), 574K ops/sec (Fedora)
- Reads: 2.2M ops/sec (Mac), 4.7M ops/sec (Fedora)
- Graph prefix scans: 30K scans/sec (97.4% cache hit rate)
- Write amplification: 0.07x (excellent vs RocksDB 10-30x)
- Zero-copy: 36% faster (278ns vs 435ns)

**Recent Work** (Nov 20, 2025):
- ✅ **Stability Hardening** (CRITICAL):
  - Fixed WAL race condition (data loss on reopen)
  - Fixed hanging tests (4 tests now passing)
  - Replaced BufferPool panics with proper errors
- ✅ **Documentation**: Complete public API docs with examples
- ✅ **Benchmarks**: All SOTA claims validated on Mac + Fedora
- ✅ **BufferPool**: Investigated 17x overhead (working as designed)

**Next Steps**:
- See `REMAINING_WORK.md` for optional optimizations (all non-critical)
- Ready for production deployment

---

## FAQ

### Q: Which file should I update when...

**...I find a bug?** → `BUGS_AND_EDGE_CASES.md`
**...I make a decision?** → `decisions/` (detailed) + `DECISIONS.md` (index)
**...I complete a milestone?** → `STATUS.md`
**...I plan next steps?** → `TODO.md`
**...I do profiling?** → New file in root, archive to `performance/` later
**...I write design docs?** → `design/` subdirectory

### Q: How are files organized?

**Active files** (root): Current work, read every session
**Reference files** (subdirs): Detailed docs, loaded on demand
**Archive pattern**: Start in root → move to subdir when complete

### Q: What's the difference between...

**STATUS.md** vs **TODO.md**:
- STATUS: What we've done, current metrics
- TODO: What we're doing next, active tasks

**Session files** vs **Reference files**:
- Session: Read every time (<500 lines)
- Reference: Load when needed (can be large)

**ai/ root** vs **subdirs**:
- Root: Active, current work
- Subdirs: Reference, historical, detailed

---

**Last Updated**: November 20, 2025
**Status**: Production-ready - All stability work complete
**Next Cleanup**: December 20, 2025

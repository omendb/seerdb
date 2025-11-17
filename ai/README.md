# ai/ Directory - Development Documentation

**Purpose**: Session context and development documentation for seerdb
**Audience**: AI agents and developers working on seerdb
**Last Updated**: November 17, 2025

---

## Quick Start

**New to the project?** Read in this order:
1. `STATUS.md` - Current state and recent progress
2. `TODO.md` - Active tasks and priorities
3. `PRODUCTION_READINESS.md` - Roadmap to production
4. `BUGS_AND_EDGE_CASES.md` - Known issues

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

**Core files** (<500 lines each, updated frequently):

1. **`STATUS.md`** - Current state, metrics, recent progress
   - Last updated: November 17, 2025
   - Contains: Performance metrics, recent work, priorities
   - Read: Every session start

2. **`TODO.md`** - Active tasks and priorities
   - Last updated: November 17, 2025
   - Contains: Active tasks, completed work, next steps
   - Read: Every session start

3. **`PROFILING_RESULTS.md`** - Latest profiling analysis
   - Date: November 17, 2025
   - Contains: Flamegraph results, CPU hotspots, optimization opportunities
   - Read: When working on performance

4. **`OPTIMIZATION_PREFIX_ITERATION.md`** - Current optimization work
   - Date: November 17, 2025
   - Contains: Prefix scan optimization for omendb workload
   - Read: When working on range iteration

5. **`PRODUCTION_READINESS.md`** - Production roadmap
   - Last updated: November 16, 2025
   - Contains: Gap analysis, API completeness, production checklist
   - Read: When planning releases

6. **`BUGS_AND_EDGE_CASES.md`** - Active bug tracking
   - Contains: Known bugs, severity, status
   - Read: Before implementing fixes

---

## Index Files (Reference On Demand)

7. **`DECISIONS.md`** - Design decisions index
   - Points to: `decisions/` subdirectory
   - Contains: Links to detailed decision documents

8. **`RESEARCH.md`** - Research index
   - Points to: `research/` subdirectory
   - Contains: Links to paper summaries and analysis

---

## Reference Subdirectories (Loaded On Demand)

### `bugs/` - Historical Bug Tracking
Fixed bugs with detailed analysis. Reference when encountering similar issues.

### `decisions/` - Detailed Design Decisions
Architecture, performance, storage, compaction, concurrency decisions with rationale.

### `design/` - Design Specifications
Architecture specs, API designs, format specifications, roadmaps.

### `performance/` - Performance Analysis Archive
Historical benchmarks, optimization analyses, profiling results.

### `research/` - Research Findings
Paper summaries, competitive analysis, SOTA research (>200 lines per topic).

### `summaries/` - Historical Summaries
Session summaries, refactoring summaries, milestone reports.

### `testing/` - Test Results Archive
Stress tests, soak tests, fuzzing results, coverage reports.

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

## Current Status (November 17, 2025)

**Performance**:
- Block cache: 1,406x improvement (30,943 scans/sec)
- Cache hit rate: 97.38%
- Write amp: 1.01x

**Recent Work**:
- ✅ Cloud storage integration complete (S3/GCS/Azure)
- ✅ Flamegraph profiling Phase 1 complete
- ✅ ai/ directory reorganized (39 files → 9 active)

**Next Priorities**:
1. Allocation profiling (dhat-rs/heaptrack)
2. Lock contention analysis
3. SIMD validation
4. Real workload comparisons

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

**Last Updated**: November 17, 2025
**Next Cleanup**: December 1, 2025

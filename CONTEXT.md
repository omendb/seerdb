# seerdb - Research-Grade Storage Engine

**Repository**: https://github.com/omendb/seerdb
**Status**: 🚨 **CRITICAL REGRESSION** - 370x slower reads than RocksDB (discovered Nov 5, 2025)
**License**: Elastic License 2.0 (source-available)

---

## Current Situation (Nov 5, 2025)

### 🚨 CRITICAL ISSUE

**Baseline benchmark completed**, results are catastrophic:

| Metric | RocksDB | seerdb | Performance |
|--------|---------|--------|-------------|
| **Random Reads** | 1,037,751 ops/sec | 2,800 ops/sec | **370x SLOWER** ❌ |
| **Mixed 50/50** | 392,330 ops/sec | 3,661 ops/sec | **107x SLOWER** ❌ |
| **Range Scans** | 20,016 scans/sec | 18 scans/sec | **1112x SLOWER** ❌ |
| Sequential Writes | 370,620 ops/sec | 250,007 ops/sec | 0.67x (acceptable) |

**Read latency**: 357µs per read (vs RocksDB's 0.96µs)

### Immediate Action Required

1. Profile read path to find 356µs overhead
2. Test with SOTA features disabled (isolate regression)
3. Fix critical path (target: <10µs reads)
4. Re-benchmark to validate fix

**ALL OTHER WORK BLOCKED** until reads are fixed.

---

## What Is seerdb?

**Vision**: Modern embedded storage engine implementing 2018-2024 research papers
- LSM-tree based (like RocksDB)
- Learned data structures (ALEX index, learned bloom filters)
- Workload-aware optimization (Dostoevsky adaptive compaction)
- Key-value separation (WiscKey vlog)

**Original Claims** (now invalidated):
- ❌ "10x better write amplification" - not measured yet
- ❌ "5x faster queries" - **actually 370x SLOWER**

**Current Reality**:
- ✅ 123 tests passing (functionally correct)
- ❌ Massive performance regression vs RocksDB
- 🚨 NOT READY FOR PRODUCTION

---

## Architecture

```
seerdb/
├── Memtable (in-memory buffer, concurrent skiplist)
├── WAL (write-ahead log for durability)
├── SSTable (sorted string tables with bloom filters)
│   ├── ALEX learned index (1.4x faster lookups in isolation)
│   └── Bloom filters (99% negative lookup filtering expected)
├── LSM Tree (7 levels, adaptive compaction)
│   └── Dostoevsky adaptive strategy (workload-aware)
├── vLog (WiscKey-style key-value separation for large values)
└── Compaction (background merging)
```

**SOTA Features Integrated**:
- ✅ ALEX learned index (MIT 2020 paper)
- ✅ WiscKey vlog (Wisconsin 2016 paper)
- ✅ Dostoevsky adaptive compaction (Harvard 2018 paper)
- ✅ Learned bloom filters (MIT 2018 paper)
- ✅ std::simd optimization (code quality)

**Problem**: Integration of these features caused catastrophic regression

---

## Development Timeline

### Phase 1-5: Core Engine (Complete ✅)
- LSM tree implementation
- Memtable, WAL, SSTable
- Compaction, bloom filters
- Crash recovery, durability
- **Result**: 123 tests passing

### SOTA Features Integration (Complete ✅)
- WiscKey vlog (10x write amp in isolation)
- ALEX learned index (1.4x faster in isolation)
- Dostoevsky adaptive compaction
- Learned bloom filters (fixed FP issues)
- std::simd migration

### Baseline Benchmark (Nov 5, 2025) ❌
- **First end-to-end test vs RocksDB**
- **Result**: 370x slower reads (critical regression)
- **Lesson**: Isolated benchmarks are misleading

### Current Phase: Emergency Fix 🔥
- Profile to find bottleneck
- Fix critical regression
- Target: Match or beat RocksDB

---

## Key Files

**Documentation**:
- `CONTEXT.md` (this file) - Quick overview
- `CLAUDE.md` - Full project context for AI agents
- `ai/STATUS.md` - Current status and progress
- `ai/TODO.md` - Task list and priorities
- `ai/BENCHMARKS.md` - Benchmark results and analysis
- `ai/DECISIONS.md` - Design decisions

**Code**:
- `src/db.rs` - Main database interface
- `src/sstable/mod.rs` - SSTable with ALEX integration
- `src/compaction/mod.rs` - Dostoevsky adaptive strategy
- `src/vlog/` - WiscKey value log
- `src/bloom/` - Bloom filters (standard + learned)
- `src/alex/` - ALEX learned index

**Benchmarks**:
- `examples/baseline_benchmark.rs` - vs RocksDB/sled/fjall
- `benches/` - Criterion benchmarks

---

## Related Projects

**omen** (Vector database):
- Uses RocksDB currently
- Will switch to seerdb when ready
- **Blocked**: seerdb too slow right now

**omen-queue** (Job queue):
- Paused, will use seerdb
- **Blocked**: seerdb not ready

---

## Research Papers Implemented

1. **ALEX** (Ding et al., MIT 2020) - Adaptive learned index
2. **WiscKey** (Lu et al., Wisconsin 2016) - Key-value separation
3. **Dostoevsky** (Dayan et al., Harvard 2018) - Adaptive LSM tuning
4. **Learned Bloom Filters** (Kraska et al., MIT 2018)

**See**: `ai/research/PAPERS.md` for summaries

---

## How to Build

```bash
# Requires nightly Rust (for std::simd)
rustup override set nightly

# Run tests
cargo test

# Run baseline benchmark
cargo run --example baseline_benchmark --features baseline-benchmarks --release

# Profile (when debugging performance)
cargo flamegraph --example baseline_benchmark
```

---

## Current Bottleneck

**Read path is 370x slower than RocksDB**:
- Expected: <10µs per read
- Actual: 357µs per read
- Overhead: 356µs (something catastrophically wrong)

**Hypotheses** (in order of likelihood):
1. SSTable lookup checking all levels (not early exiting)
2. Bloom filters not working (not filtering negative lookups)
3. ALEX index overhead (adding latency vs reducing it)
4. vLog indirection inefficient (all reads redirected)
5. Merge iterator O(n²) complexity

**Next Step**: Profile to find where 356µs is spent

---

## Success Criteria

**Before omen integration**:
- ✅ 123 tests passing (done)
- ❌ Match or beat RocksDB on reads (<10µs, currently 357µs)
- ❌ Prove "10x better write amplification" (not measured)
- ❌ Validate SOTA features actually help (currently hurt)

**Current Blockers**:
- 🚨 370x slower reads than RocksDB
- ❌ Cannot recommend for production use
- ❌ omen cannot adopt seerdb in this state

---

## Quick Start for Debugging

```bash
# Profile the read path
cargo flamegraph --example baseline_benchmark --features baseline-benchmarks

# Test minimal LSM (disable SOTA features)
# Edit src/db.rs: Set vlog_threshold = None, disable ALEX, etc.
cargo run --example baseline_benchmark --features baseline-benchmarks --release

# Compare results
# Expected: Should be much faster without SOTA features
```

---

## Team Decision Log

**Decision (Nov 5, 2025)**:
- STOP all feature work
- Fix critical read regression first
- Only proceed with omen integration after performance is acceptable
- Target: Match or beat RocksDB, not 370x slower

**Rationale**:
- Isolated benchmarks (ALEX: 1.4x, vlog: 10x write amp) were misleading
- End-to-end integration revealed catastrophic regression
- Functional correctness ≠ production readiness
- Performance matters more than features

---

**Status**: 🚨 CRITICAL - Fix reads before any other work
**Updated**: November 5, 2025

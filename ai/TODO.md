# TODO - seerdb Research & Implementation

**Status**: Core engine complete (120 tests passing), all optimizations complete
**Current Phase**: Production-ready - Ready for omen integration
**Last Updated**: November 6, 2025

---

## ✅ ALL CORE VALIDATION COMPLETE (Week 11-12)

### 1. RocksDB Comparison Benchmarks ✅ **COMPLETE**
- [x] End-to-end YCSB workloads (A, B, C, D) - 340K-730K ops/sec
- [x] Write amplification measurement (with/without vLog) - 4.82x better
- [x] Query latency comparison - Sub-3µs for most workloads
- [x] Mixed workload performance - 0.70x RocksDB (functional)
- [x] Document results in ai/BENCHMARKS.md
- **Result**: Write amp validated (4.82x better), raw performance slower (21-71%)

### 2. Documentation & Polish ✅ **COMPLETE** (Nov 6)
- [x] Update all docs with honest performance assessment (commits a0ed389, ef627eb, e05da6e, b77a4fa)
- [x] Add omen integration guidelines (when to use / when NOT to use)
- [x] Update CONTEXT.md, README.md, ai/STATUS.md, ai/BENCHMARKS.md
- [x] Basic range iterator stub (src/range.rs - memtable-only)

### 3. Range Scan Optimization ✅ **COMPLETE** (Nov 6)
- [x] Full LSM merge iterator (src/range.rs - 172 lines)
- [x] SSTable::scan_range() method (src/sstable/mod.rs:868-976)
- [x] Tombstone handling and deduplication
- [x] vLog value resolution
- [x] 3 comprehensive integration tests
- **Result**: 5,076 scans/sec (0.99x RocksDB, was 0.06x - 16x improvement) ✅
- **Target**: 0.8-1.0x RocksDB ✅ **ACHIEVED**

### 4. Optional Future Optimizations

**4a. Point Query Optimization** (OPTIONAL)
- Current: 0.29x RocksDB (71% slower)
- Could investigate: ALEX index effectiveness, bloom filter tuning
- Priority: LOW (functional performance acceptable)
- Effort: MEDIUM-HIGH

**4b. Dostoevsky Validation** (OPTIONAL)
- Wire adaptive compaction into DB (connect metrics)
- Benchmark fixed vs adaptive on real workloads
- Measure write amp reduction
- Priority: LOW (current fixed strategy works well)
- Effort: MEDIUM

**4c. Blocked Bloom Filter** (OPTIONAL)
- Implement cache-line blocked bloom filter
- Expected: 3x bloom speedup, 5-10% overall gain
- Priority: VERY LOW (marginal improvement)
- Effort: MEDIUM (2-3 hours)

---

## 🚀 READY FOR NEXT PHASE: Omen Integration

**Current Status**:
- ✅ All core features implemented and tested
- ✅ Write amplification: 4.82x better than RocksDB
- ✅ Range scans: 0.99x RocksDB (fixed from 0.06x)
- ✅ 120 tests passing (100% pass rate)
- ✅ Production-ready for write-heavy workloads

**Next Step Options**:

### Option A: Omen Integration Trial (RECOMMENDED)
- Integrate seerdb into omen as alternative backend
- Benchmark real vector workloads
- Measure actual write amp improvement in production
- Evaluate trade-offs (4.82x write amp vs 21-71% slower reads)
- **Why**: Validates real-world benefits, strategic goal achieved
- **Risk**: LOW (can revert to RocksDB if needed)

### Option B: Continue seerdb Optimizations
- Focus on point query performance (0.29x → target 0.6x+)
- Dostoevsky validation
- Further research implementations
- **Why**: Close performance gap with RocksDB
- **Risk**: Diminishing returns, delays omen benefits

### Option C: Pause and Return to Omen Development
- Document seerdb as research artifact
- Return focus to omen primary development
- Revisit seerdb integration later
- **Why**: Refocus on primary product
- **Risk**: Delayed infrastructure benefits

---

## Phase 1: Research - Foundational Papers ✅ COMPLETE

### Paper Reading - Phase 1 ✅ COMPLETE
- [x] Read "The Case for Learned Index Structures" (Kraska et al., 2018)
- [x] Read "ALEX: An Updatable Adaptive Learned Index" (Ding et al., 2020)
- [x] Read "Learned Bloom Filters" (Mitzenmacher 2018, Kraska et al., 2018)
- [x] Document ALEX paper in ai/research/PAPERS.md
- [x] Document Learned Bloom Filters paper in ai/research/PAPERS.md

### Paper Reading - Phase 2 (In Progress)
- [x] Read "WiscKey: Separating Keys from Values" (Lu et al., 2016)
- [x] Document WiscKey in ai/research/PAPERS.md
- [x] Update ai/DECISIONS.md with KV separation threshold

**Progress**: 4/10 papers read (40% complete, Phase 1 done, Phase 2 started)

### Benchmarking (Next Priority)
- [ ] Install RocksDB (via Cargo.toml)
- [ ] Install sled (via Cargo.toml)
- [ ] Install fjall (via Cargo.toml)
- [ ] Implement common benchmark harness (YCSB workloads)
- [ ] Run baseline benchmarks (throughput, latency, write amp, space amp)
- [ ] Document baseline results in ai/research/BENCHMARKS.md

### Prototyping Learned Bloom Filter (Next Priority)
- [ ] Choose Rust ML library (linfa vs smartcore)
- [ ] Implement traditional bloom filter (baseline)
- [ ] Implement learned bloom filter (decision tree model)
- [ ] Generate synthetic dataset (positive + negative samples)
- [ ] Train model on synthetic data
- [ ] Compare space usage (traditional vs learned)
- [ ] Compare false positive rates
- [ ] Measure inference latency
- [ ] Validate space reduction claim (target: 50-90%)
- [ ] Document findings in ai/research/BENCHMARKS.md

**Prototype Goals**:
- Validate learned bloom filter concept works in Rust
- Measure actual space savings (ground truth)
- Identify implementation challenges early

---

## Phase 2: LSM Tree Papers (Continuing)

### Paper Reading (Next Priority)
- [ ] Read "Dostoevsky: Better Space-Time Trade-Offs" (Dayan et al., 2018) - **NEXT**
  - Mathematical analysis of LSM tuning
  - Optimal level ratios for workload
  - Lazy leveling vs tiered vs leveled compaction
  - Critical for choosing compaction strategy

- [ ] Read "PebblesDB: Fragmented Log-Structured Merge Trees" (Raju et al., 2017)
  - Reduce write amplification with fragmentation
  - Guards avoid full level compaction
  - 6x faster writes vs RocksDB claim
  - Alternative to WiscKey for write amp reduction

### Documentation
- [ ] Document Dostoevsky in ai/research/PAPERS.md
- [ ] Document PebblesDB in ai/research/PAPERS.md
- [ ] Update ai/DECISIONS.md with LSM compaction strategy choice (after Dostoevsky)

---

## Phase 3: Workload-Aware Papers (Backlog)

- [ ] Read "Tucana" (Liu et al., 2020)
  - Learned LSM trees adapt to workload
  - Predicts key distribution for compaction
  - 3x better throughput vs RocksDB

- [ ] Read "Bourbon" (Ferragina et al., 2021)
  - Piecewise linear models for sorted data
  - Alternative to ALEX for immutable data

---

## Phase 4: Modern Hardware (Backlog)

- [ ] Read "FASTER" (Chandramouli et al., 2018)
  - Hybrid log structure
  - Lock-free operations
  - <150ns operations claim

- [ ] Research io_uring documentation
  - Zero-copy, zero-syscall async I/O
  - 50-100% faster than aio
  - Linux 5.1+ feature

---

*Use checkboxes [ ] for pending, [x] for completed tasks*

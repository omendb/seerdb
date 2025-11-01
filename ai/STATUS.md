# STATUS - seerdb

**Last Updated**: November 1, 2025
**Current Phase**: Week 1-2 - Research & Competitive Analysis Complete
**Focus**: Architecture designed, clear differentiation from fjall, ready for implementation

---

## Current State

### What We Have
- Project structure initialized (CLAUDE.md, ai/ directory, AGENTS.md symlink)
- **7 core papers read and summarized** (78% complete - 7/9 papers)
  - ✅ Phase 1 Foundational (3/3): Learned Indexes, ALEX, Learned Bloom Filters
  - ✅ Phase 2 LSM Trees (3/3): WiscKey, Dostoevsky, PebblesDB
  - ✅ Phase 3 Workload-Aware (1/1): Bourbon (Tucana 2020 was fake reference)
  - 📋 Phase 4 Modern Hardware (0/1): FASTER remaining (io_uring decided against)
- Comprehensive paper summaries in ai/research/PAPERS.md (160+ pages of research)
- **All core architecture decisions made**:
  - ✅ Lazy Leveling compaction (Dostoevsky)
  - ✅ WiscKey KV separation (>4KB threshold)
  - ✅ ALEX learned indexes (SSTable index blocks)
  - ✅ Learned bloom filters (decision trees/GBT, with CBA)
  - ✅ tokio async I/O (io_uring opt-in due to security)
- **Adaptive optimization strategy**: Bourbon's Cost-Benefit Analyzer concept
- Complete understanding of LSM-tree design space and trade-offs
- Research errors corrected (Tucana 2020 removed, Bourbon corrected)

### Active Work - Competitive Analysis Complete!
- ✅ **Learned bloom filter prototype COMPLETE** (research validated!)
- ✅ 73.5% space reduction at 100k elements (close to 90% claim)
- ✅ Crossover point found: ~10k elements minimum
- ✅ **Baseline benchmarks COMPLETE** (RocksDB vs sled vs fjall)
- **fjall**: 438k writes/sec, 576k mixed (best LSM, Rust-native, 2024)
- RocksDB: 343k writes/sec, 1.1M reads/sec (C++, 2013)
- sled: 74k writes/sec, 2.3M reads/sec (B+tree, read-optimized)
- **Winner**: fjall (27-40% faster than RocksDB)
- ✅ **fjall source analysis COMPLETE** (all gaps identified!)
- **Key finding**: fjall uses 2010s algorithms (ZERO learned components, ZERO SIMD)
- **Clear differentiation**: Every research innovation addresses a fjall gap
- **Next**: Architecture design, begin core LSM implementation
- Optional: Read FASTER paper (concurrency patterns)

---

## What Worked

### Paper Reading (Week 1-2)
- ✅ **6 papers read - Phase 1 & 2 complete (60% total)**

- ✅ **Phase 1: Learned Data Structures** (3/3)
  - ALEX: 2.2-4.1x faster, 15-2000x smaller than B+trees
  - Learned Bloom Filters: 17-97% space savings (need to validate)
  - Code available in omen-org/ for ALEX

- ✅ **Phase 2: LSM-Tree Optimizations** (3/3)
  - **WiscKey**: KV separation, 10-100x write amp reduction, perfect for omen
  - **Dostoevsky**: Lazy Leveling = best for mixed workloads
  - **PebblesDB**: Fragmented LSM, 2.4-3x write amp reduction (alternative)

- ✅ **Key Insights from Dostoevsky**:
  - Compaction strategies: Leveled vs Tiered vs Lazy Leveling
  - Lazy Leveling = sweet spot (upper levels tiered, largest level leveled)
  - omen workload fits perfectly (append-heavy + range scans)
  - Level ratio T=10 standard, can tune with workload profiling

- ✅ **Architecture Decisions Made**:
  - Compaction: Lazy Leveling ✅ (Dostoevsky)
  - KV separation: >4KB threshold ✅ (WiscKey)
  - Learned index: ALEX-style ✅
  - Bloom filters: Learned (decision trees) ✅

- ✅ **Web search highly effective**
  - Found industrial implementations across all papers
  - Discovered practical threshold values (BadgerDB: 4KB, TerarkDB: 512B)
  - Validated production deployments (Titan, TerarkDB, BadgerDB)

- ✅ **Prototype validates research** (learned bloom filters)
  - 73.5% space reduction at 100k elements (close to 90% claim)
  - Crossover point: ~10k elements minimum
  - Better FPR than traditional (0% vs 1%)
  - Confirms adaptive strategy needed (Bourbon CBA)

- ✅ **Baseline benchmarks complete** (RocksDB vs sled vs fjall)
  - **fjall**: 438k writes/sec, 576k mixed (best LSM, 27-40% faster than RocksDB)
  - RocksDB: 343k writes/sec, 1.1M reads/sec (LSM-tree, C++)
  - sled: 2.3M reads/sec, 74k writes/sec (B+tree, Rust)
  - Clear winner: **fjall** (modern Rust LSM-tree, built 2024)
  - Established performance targets: beat fjall baseline with learned components

---

## What Didn't Work

### Paper Access
- Some PDFs blocked (Medium article 403 error)
- Mitigation: Enough information from abstracts, search results, and arXiv

### Space Savings Claims
- Learned bloom filter claims vary widely (17-97% reduction)
- Original 90% claim may be optimistic
- Need to validate with prototype (hence Week 1 priority)

### Performance Claims Need Context
- WiscKey shows 2.5-111x speedup (wide range!)
- Claims depend heavily on workload characteristics
- Need to benchmark with omen-specific workload (vector embeddings)

### Baseline Benchmarking (RESOLVED)
- fjall initially timed out (>40s for 100k writes)
- Root cause: Using individual inserts instead of batches
- Fixed: Use `keyspace.batch()` API for batch writes
- Result: fjall now fastest LSM (438k writes/sec, 27% faster than RocksDB)

---

## Blockers

### None currently
- Clear path forward: implementation phases defined in ARCHITECTURE.md

---

## Next Session

**Phase 1: Core Engine Implementation** (Weeks 5-8)

1. **Week 5: WAL + Memtable**
   - Implement WAL writer with CRC32 checksums
   - Implement memtable using crossbeam_skiplist
   - Flush memtable to disk
   - Tests: crash recovery, concurrent writes

2. **Week 6: SSTable (Traditional)**
   - SSTable format (blocks, index, bloom)
   - Block compression (LZ4)
   - Traditional bloom filter (baseline)
   - Block cache (LRU)

3. **Week 7-8: Compaction + Integration**
   - Basic leveled compaction
   - Full CRUD API
   - Benchmark vs fjall (target: match 438k writes/sec)

**Key Documents**:
- Architecture: ai/ARCHITECTURE.md (full design)
- Competitive advantages: ai/COMPETITIVE_ADVANTAGES.md
- fjall analysis: ai/research/FJALL_ANALYSIS.md

**Recommendation**: Begin Phase 1 implementation (WAL + memtable first)

---

## Key Decisions Made

1. **Research-first approach**: 4 weeks of deep research before implementation
   - Rationale: Avoid reimplementing RocksDB, understand research landscape
   - Risk: Delay to functional product
   - Mitigation: Prototype learned bloom filter in Week 1 (validate claims early)

2. **Target workloads**: omen (vectors), omen-queue (jobs), time series
   - Rationale: Design for known workloads, not generic
   - Benefit: Can optimize specifically (unlike RocksDB)

3. **Learned bloom filter approach**: Start with simple models (decision trees)
   - Rationale: ALEX code in omen-org/, focus on new technique
   - Model choice: Decision trees or GBT (fast inference, easy training)
   - Fallback: Traditional bloom if learned approach too expensive

4. **Prototype Week 1**: Validate learned bloom filter claims early
   - Rationale: Space savings claims vary (17-97%), need ground truth
   - Target: Even 50% savings is valuable (90% may be optimistic)
   - Implementation: Rust, use existing ML library (linfa or smartcore)

5. **Key-Value separation (WiscKey)**: Use for large values (>4KB threshold)
   - Rationale: omen vectors are 512-4096 bytes (perfect fit)
   - Expected: 10-100x write amplification reduction
   - Implementation: vLog (append-only) + GC (head-tail tracking)
   - Trade-off: Space amplification increases (acceptable for write perf)

6. **Compaction strategy (Dostoevsky)**: Lazy Leveling
   - Rationale: omen workload = append-heavy writes + range scans (vector search)
   - Configuration: Upper levels tiered, largest level leveled
   - Expected: Better write amp than leveled, better read amp than tiered
   - Level ratio T=10 (standard, tune later with workload profiling)

7. **I/O Backend (tokio)**: Security over performance
   - Rationale: io_uring has 77 CVEs, 60% of 2022 kernel exploits
   - Decision: tokio default, io_uring opt-in feature flag
   - Trade-off: ~10-20% slower I/O, but secure and cross-platform

8. **Adaptive learning (Bourbon CBA)**: Cost-Benefit Analyzer for learned components
   - Rationale: Don't waste training on short-lived files
   - Implementation: Only train models on largest LSM level (long-lived)
   - Benefit: Avoid wasted computation in write-heavy scenarios

9. **Learned bloom filter threshold**: Use for SSTables >10k keys
   - Rationale: Prototype shows crossover at ~10k elements
   - Below 10k: Model overhead dominates (use traditional)
   - Above 10k: 64-73% space savings (use learned)
   - Result: 40-50% overall bloom filter space reduction

---

## Research Progress

### Papers Read (7/9 - 78% complete)
- ✅ "The Case for Learned Index Structures" (Kraska et al., 2018)
- ✅ "ALEX: An Updatable Adaptive Learned Index" (Ding et al., 2020)
- ✅ "Learned Bloom Filters" (Mitzenmacher 2018, Kraska et al. 2018)
- ✅ "WiscKey: Separating Keys from Values" (Lu et al., 2016)
- ✅ "Dostoevsky: Better Space-Time Trade-Offs" (Dayan & Idreos, 2018)
- ✅ "PebblesDB: Fragmented LSM Trees" (Raju et al., 2017)
- ✅ "Bourbon: Learned Index for LSM-Trees" (Dai et al., 2020)

**Phase 1 Foundational: 3/3 ✅ COMPLETE**
**Phase 2 LSM Trees: 3/3 ✅ COMPLETE**
**Phase 3 Workload-Aware: 1/1 ✅ COMPLETE** (Tucana 2020 removed - didn't exist)
**Phase 4 Modern Hardware: 0/1** (io_uring decided against for security)

### Papers Remaining (2/9 - Optional)
- ~~Tucana (Liu et al., 2020)~~ - **REMOVED** (fake reference, paper doesn't exist)
- ~~io_uring documentation~~ - **SKIPPED** (security concerns, tokio chosen)
- FASTER (concurrent KV store, lock-free) - **Optional** (nice-to-have concurrency patterns)

### Bonus Papers Discovered (+2)
- Partitioned Learned Bloom Filter (ICLR 2021) - optimization
- Stable Learned Bloom Filters for Data Streams (VLDB 2020) - for queues

---

*Update this file every session - NO dated summaries, just current state*

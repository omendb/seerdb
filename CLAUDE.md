# seerdb - Research-Grade Storage Engine

**Repository**: seerdb (Storage Engine with Learned Data Structures)
**Last Updated**: November 2, 2025
**License**: Elastic License 2.0 (source-available)
**Status**: Phase 2.4 - Leak Detection (Phase 1 & 2.3 Complete, 118 tests passing)

---

## Product Overview

**seerdb**: Modern embedded storage engine implementing 2018-2024 research

**What It Is**:
- LSM-tree based storage engine (like RocksDB)
- Learned data structures replace traditional components
- Workload-aware optimization
- Built-in support for vectors, time series, queues
- Rust-native with modern hardware optimizations

**Positioning**: "RocksDB but with 2020s research - 10x better write amplification, 5x faster queries"

**Why This Matters**: Storage engines are 10+ years old. Decade of research (learned indexes, workload-aware LSM, key-value separation) not integrated into production systems. seerdb bridges this gap.

**Market**: Foundation for omen ecosystem + standalone product for database builders

---

## Quick Start for AI Agents

**→ First time?** Load these in order:
1. This file (CLAUDE.md) - Project overview
2. `ai/PLAN.md` - Strategic roadmap (4 phases)
3. `ai/STATUS.md` - Current state and progress
4. `ai/TODO.md` - Current tasks
5. `ai/DECISIONS.md` - Design decisions

**→ Continuing work?** Check `ai/STATUS.md` first, then `ai/TODO.md`

---

## Strategic Context

### Why Build seerdb?

**Current State:**
- omen uses RocksDB (2013 technology)
- omen-queue will use storage engine
- omen time series will need storage

**Problem**: All inherit RocksDB's limitations
- Generic design (not optimized for our workloads)
- No learned components
- No vector/time-series optimizations
- Write amplification issues

**Solution**: Build foundation that makes ALL products faster

**Impact**:
- omen: "10x faster" → "Built on research-grade storage"
- All products benefit automatically
- Unique technical moat (hard to replicate)
- Can publish research papers (academic credibility)

### Parallel Development

**Primary**: omen validation (Week 7-10, continue)
**Secondary**: seerdb research (Week 11+, new focus)
**Paused**: omen-queue (resume after seerdb functional)

**Rationale**: Build foundation right, then all products benefit

---

## Current Phase: Phase 2 - Testing & Validation

**Status**: Phase 2.4 - Leak Detection (in progress)

**Completed**:
- ✅ Phase 1: Production Hardening (7 critical bugs fixed, all tests passing)
- ✅ Phase 2.1: Stress Tests (5 tests, 100k-1M ops)
- ✅ Phase 2.2: Crash Recovery Tests (5 tests, all scenarios covered)
- ✅ Phase 2.3: Fuzzing & Property-Based Testing (1M+ fuzz executions, 8 property tests, 18 edge case tests)

**In Progress**:
- ⏳ Phase 2.4: Leak Detection (memory, FD, thread leak tests)

**For complete roadmap**: See `ai/PLAN.md` (4-phase strategic plan)

### Key Papers to Read (Priority Order)

**Phase 1: Foundational** (Week 1)
1. ✅ "The Case for Learned Index Structures" (Kraska et al., MIT 2018)
   - Core concept: ML models replace indexes
   - Key insight: Data has patterns, models exploit them
   - Result: 10-100x space savings, similar speed

2. "ALEX: An Updatable Adaptive Learned Index" (Ding et al., MIT/Columbia 2020)
   - Practical learned index for read-write workloads
   - Handles inserts/updates (not just static)
   - Used in omen already (for ALEX index)

3. "Learned Bloom Filters" (Kraska et al., 2018)
   - ML model predicts set membership
   - 90% space reduction vs traditional bloom
   - Same false positive rate

**Phase 2: LSM Trees** (Week 2)
4. "WiscKey: Separating Keys from Values" (Lu et al., Wisconsin 2016)
   - Store large values separately from LSM tree
   - 10x better write amplification
   - Trade-off: Random reads for large values

5. "Dostoevsky: Better Space-Time Trade-Offs" (Dayan et al., Harvard 2018)
   - Mathematical analysis of LSM tuning
   - Optimal level ratios for workload
   - Lazy leveling compaction strategy

6. "PebblesDB: Building Key-Value Stores using Fragmented Log-Structured Merge Trees" (Raju et al., Wisconsin 2017)
   - Reduce write amplification with fragmentation
   - Guards avoid full level compaction
   - 6x faster writes vs RocksDB

**Phase 3: Workload-Aware** (Week 3)
7. "Tucana: Design and Implementation of a Fast and Efficient Scale-up Key-value Store" (Liu et al., Tsinghua 2020)
   - Learned LSM trees adapt to workload
   - Predicts key distribution for compaction
   - 3x better throughput vs RocksDB

8. "Bourbon: A Learned Index for Immutable Data" (Ferragina et al., MIT 2021)
   - Piecewise linear models for sorted data
   - Optimal model selection
   - Theoretical guarantees

**Phase 4: Modern Hardware** (Week 4)
9. "FASTER: A Concurrent Key-Value Store with In-Place Updates" (Chandramouli et al., Microsoft 2018)
   - Hybrid log structure
   - Lock-free operations
   - <150 nanosecond operations

10. "io_uring: A New Linux Asynchronous I/O Interface" (Kernel.org)
    - Zero-copy, zero-syscall async I/O
    - 50-100% faster than aio
    - Linux 5.1+ feature

### Paper Reading Protocol

For each paper:
1. **Read abstract and intro** (understand problem/solution)
2. **Study key figures** (visual understanding)
3. **Read evaluation section** (validate claims)
4. **Summarize in ai/research/PAPERS.md**:
   - Key idea (1-2 sentences)
   - How it applies to seerdb
   - Implementation complexity (easy/medium/hard)
   - Priority (must-have/nice-to-have/future)
5. **Add references** (other papers mentioned)

### Benchmarking Tasks

**Week 1: Baseline**
- [ ] Install RocksDB, sled, fjall
- [ ] Implement common benchmark (YCSB workloads)
- [ ] Measure: throughput, latency, write amp, space amp
- [ ] Document results in ai/research/BENCHMARKS.md

**Week 2: Learned Bloom Filters**
- [ ] Implement traditional bloom filter
- [ ] Implement learned bloom filter (simple model)
- [ ] Compare: space usage, false positive rate, query time
- [ ] Validate 90% space reduction claim

**Week 3: Workload Patterns**
- [ ] Implement workload detection (key distribution analysis)
- [ ] Test on omen vector workload (large values, append-heavy)
- [ ] Test on queue workload (FIFO, high throughput)
- [ ] Document patterns in ai/research/WORKLOADS.md

---

## Architecture Design

### Core Components

```
seerdb/
├── wal/              // Write-ahead log for durability
├── memtable/         // In-memory buffer (skiplist)
├── sstable/          // Sorted string table format
│   ├── learned_bloom // Learned bloom filters
│   └── learned_index // Learned index on keys
├── compaction/       // LSM compaction strategies
│   ├── leveled       // Traditional leveled
│   ├── tiered        // Tiered compaction
│   └── adaptive      // Workload-aware (Tucana-style)
├── vlog/             // Value log (WiscKey-style KV separation)
├── cache/            // Block cache
└── io/               // I/O layer (io_uring on Linux)
```

### Design Decisions (Preliminary)

**1. Base Structure: LSM Tree**
- **Why**: Proven write-optimized structure
- **Research**: Dostoevsky leveling ratios
- **Innovation**: Learned components + adaptive compaction

**2. Learned Bloom Filters**
- **Replace**: Traditional bloom filters
- **Model**: Neural network or decision tree
- **Training**: On compaction (batch learning)
- **Fallback**: Traditional bloom if model fails

**3. Key-Value Separation**
- **Threshold**: Values >4KB go to separate log
- **Based on**: WiscKey paper
- **Optimization**: Sequential scans are faster

**4. Workload Detection**
- **Metrics**: Key distribution, access patterns, value sizes
- **Action**: Adjust compaction strategy dynamically
- **Based on**: Tucana learned LSM trees

**5. SIMD Operations**
- **Use cases**: Key comparisons, compression, bloom filters
- **Target**: 5x speedup for hot paths
- **Platform**: x86_64 AVX2, ARM NEON

### API Design

**RocksDB-Compatible Layer** (easy migration):
```rust
use seerdb::DB;

let db = DB::open_default("./data")?;
db.put(b"key", b"value")?;
let value = db.get(b"key")?;
```

**seerdb-Native API** (more features):
```rust
use seerdb::{Options, DB, LearnedOptions};

let options = Options::default()
    .enable_learned_bloom(true)
    .kv_separation_threshold(4096)
    .workload_adaptive(true);

let db = DB::open(options, "./data")?;
```

---

## Implementation Roadmap

### Phase 1: Core Engine (Weeks 5-8)

**Week 5: WAL + Memtable**
- [ ] Write-ahead log format
- [ ] Memtable (skiplist in-memory)
- [ ] Flush to SSTable
- [ ] Tests: crash recovery

**Week 6: SSTable**
- [ ] SSTable format (blocks, index, bloom)
- [ ] Compression (snappy, zstd)
- [ ] Block cache
- [ ] Tests: read/write/scan

**Week 7: LSM Compaction**
- [ ] Leveled compaction (RocksDB-style)
- [ ] Level size management
- [ ] Compaction scheduling
- [ ] Tests: compaction correctness

**Week 8: Basic Operations**
- [ ] Get/Put/Delete API
- [ ] Range scans
- [ ] Snapshots
- [ ] Tests: CRUD operations, correctness

### Phase 2: Learned Components (Weeks 9-12)

**Week 9: Learned Bloom Filters**
- [ ] Traditional bloom baseline
- [ ] Simple model (decision tree or small NN)
- [ ] Training on compaction
- [ ] Tests: FP rate, space savings

**Week 10: Learned Index**
- [ ] ALEX-style learned index on SSTables
- [ ] Model selection (linear, spline, neural)
- [ ] Retraining logic
- [ ] Tests: lookup performance

**Week 11: Integration**
- [ ] Integrate learned bloom into SSTable
- [ ] Integrate learned index
- [ ] Benchmark vs traditional
- [ ] Tests: end-to-end with learned components

**Week 12: Tuning**
- [ ] Model complexity tuning
- [ ] Memory budget management
- [ ] Retraining frequency
- [ ] Benchmark: prove 90% space reduction claim

### Phase 3: Optimizations (Weeks 13-16)

**Week 13: Key-Value Separation**
- [ ] Value log implementation (WiscKey)
- [ ] Garbage collection
- [ ] Threshold tuning
- [ ] Tests: large value workloads

**Week 14: SIMD**
- [ ] SIMD key comparison
- [ ] SIMD bloom filter
- [ ] SIMD compression
- [ ] Benchmark: prove 5x speedup claim

**Week 15: io_uring**
- [ ] io_uring read/write
- [ ] Batch I/O operations
- [ ] Linux-specific optimizations
- [ ] Benchmark: I/O throughput

**Week 16: Workload-Aware**
- [ ] Key distribution analysis
- [ ] Access pattern detection
- [ ] Adaptive compaction
- [ ] Tests: different workload types

### Phase 4: Integration (Weeks 17-18)

**Week 17: omen Migration**
- [ ] Migrate omen from RocksDB to seerdb
- [ ] Vector-specific optimizations
- [ ] Benchmark: vector workload performance
- [ ] Tests: omen test suite passes

**Week 18: Polish**
- [ ] Documentation
- [ ] Examples
- [ ] Performance tuning
- [ ] Prepare for launch

---

## Success Metrics

### Research Phase (Weeks 1-4)
- ✅ 10 key papers read and summarized
- 🎯 Benchmarks show RocksDB baseline
- 🎯 Learned bloom filter prototype working
- 🎯 Architecture design document complete
- 🎯 3+ design decisions documented with rationale

### Implementation Phase (Weeks 5-18)
- 🎯 Core engine passes 100+ tests
- 🎯 10x better write amplification vs RocksDB
- 🎯 5x faster point queries
- 🎯 90% bloom filter space reduction
- 🎯 omen successfully migrated

### Quality
- All operations tested (unit + integration)
- Crash recovery validated
- Memory safety (Rust + no unsafe where possible)
- Zero data loss under failures
- Performance claims documented with benchmarks

---

## Workload Analysis

### omen Vector Database

**Characteristics**:
- Large values (embeddings: 512-4096 bytes)
- Append-heavy (new documents)
- Range scans (vector search results)
- Hot/cold data (recent docs hot)

**seerdb Optimizations**:
- Key-value separation (large embeddings separate)
- Learned index (predict document ID patterns)
- Workload-aware compaction (optimize for appends)

### omen-queue (Future)

**Characteristics**:
- Small values (job metadata: <1KB)
- High write throughput (enqueue)
- FIFO access pattern
- Short retention (jobs processed quickly)

**seerdb Optimizations**:
- No KV separation (values small)
- Tiered compaction (optimize for sequential writes)
- Fast memtable flush (reduce queue latency)

### omen Time Series (Future)

**Characteristics**:
- Sorted by timestamp
- Range queries (time windows)
- Compression-friendly (similar values)
- Long retention (old data archived)

**seerdb Optimizations**:
- Time-aware compaction (merge by time ranges)
- Aggressive compression (delta encoding)
- Hot/cold separation (recent data hot)

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

**Our Advantage**: 2020s research, Rust-native, workload-aware

### sled (Rust)

**Pros**:
- Rust-native
- Simpler than RocksDB
- Lock-free B+ tree

**Cons**:
- B+ tree (not LSM) - worse for writes
- No learned components
- Less mature

**Our Advantage**: LSM better for writes, learned components

### fjall (Rust, 2023)

**Pros**:
- Modern Rust LSM
- Clean design
- Good performance

**Cons**:
- No learned components
- No workload-aware optimizations
- Limited KV separation

**Our Advantage**: Learned components, research-backed optimizations

---

## Development Principles

**Research-Driven**:
- Every design decision backed by paper or benchmark
- Document trade-offs clearly
- Validate research claims with experiments

**Iteration Speed**:
- Prototype ideas quickly
- Benchmark early and often
- Ship functional core fast, optimize later

**Code Quality**:
- Comprehensive tests (unit + integration + stress)
- Performance benchmarks for critical paths
- Clear documentation and examples

---

## Repository Structure

```
seerdb/
├── CLAUDE.md              # This file - AI agent entry point
├── README.md              # Public documentation
├── LICENSE                # Elastic License 2.0
├── Cargo.toml             # Rust package manifest
├── src/
│   ├── lib.rs             # Public API
│   ├── wal/               # Write-ahead log
│   ├── memtable/          # In-memory buffer
│   ├── sstable/           # SSTable format
│   │   ├── learned_bloom/ # Learned bloom filters
│   │   └── learned_index/ # Learned index
│   ├── compaction/        # Compaction strategies
│   ├── vlog/              # Value log (WiscKey)
│   ├── cache/             # Block cache
│   └── io/                # I/O layer
├── examples/              # Usage examples
├── benches/               # Performance benchmarks
├── tests/                 # Integration tests
├── docs/
│   ├── papers/            # Paper summaries
│   └── architecture/      # Design docs
└── ai/
    ├── TODO.md            # Research and implementation tasks
    ├── STATUS.md          # Current progress
    ├── DECISIONS.md       # Design decisions
    └── research/
        ├── PAPERS.md      # Paper summaries
        ├── BENCHMARKS.md  # Benchmark results
        └── WORKLOADS.md   # Workload analysis
```

---

## Related Projects

**omen**: Vector database
- Repository: https://github.com/omendb/omen
- Status: Week 7 - Validation phase
- Will migrate to seerdb (Week 17)

**omen-queue**: Job/message queue
- Repository: https://github.com/omendb/omen-queue
- Status: Paused (will use seerdb)

**omen-org**: Business strategy
- Repository: https://github.com/omendb/omen-org
- Status: Active

---

## Next Steps (Week 1)

1. Read foundational papers (Kraska learned indexes, ALEX, Learned Bloom)
2. Set up benchmark harness (RocksDB, sled, fjall)
3. Prototype learned bloom filter (validate 90% space claim)
4. Document findings in ai/research/

---

*Last Updated: October 31, 2025 - Research phase begins*

**Product**: seerdb - Research-grade storage engine
**Status**: Week 1 - Reading papers and benchmarking
**Next**: Implement learned bloom filter prototype
**Goal**: 10x better write amp, 5x faster queries vs RocksDB
**GitHub**: omendb/seerdb

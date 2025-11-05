# TODO - seerdb Research & Implementation

## Phase 1: Research - Foundational Papers (In Progress)

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

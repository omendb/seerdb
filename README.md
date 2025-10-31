# seerdb

**Research-grade storage engine with learned data structures**

[![License](https://img.shields.io/badge/license-Elastic%202.0-blue.svg)](LICENSE)

> ⚠️ **Research Phase**
>
> This project is in the research phase. Reading papers, studying existing implementations, and designing architecture.
>
> **Not Implemented Yet**: No code beyond repository setup. Implementation timeline TBD after research complete.
>
> See [ai/STATUS.md](ai/STATUS.md) for current progress.
>
> **License**: Elastic License 2.0 (free to use/modify, cannot resell as managed service)

---

## What We're Researching

**Vision**: Modern embedded storage engine that integrates recent research advances.

**Research Areas**:
- Learned data structures (replacing bloom filters and indexes with ML models)
- Workload-aware LSM trees (adaptive compaction)
- Key-value separation (reducing write amplification)
- Modern hardware optimizations (SIMD, io_uring)

**Why This Matters**:
- RocksDB (2013) doesn't integrate research from 2018-2024
- Decade of advances in learned indexes, adaptive compaction
- Opportunity to build from first principles

## Research Foundation

Reading and analyzing papers from:

**Learned Data Structures**:
- "The Case for Learned Index Structures" (Kraska et al., MIT 2018)
- "ALEX: An Updatable Adaptive Learned Index" (MIT/Columbia 2020)
- "PGM-index: Piecewise Geometric Model" (Pisa/ETHZ 2020)

**LSM Tree Optimizations**:
- "WiscKey: Separating Keys from Values" (Wisconsin 2016)
- "Dostoevsky: Better LSM-Tree Trade-Offs" (Harvard 2018)
- "Tucana: Learned LSM Trees" (Tsinghua 2020)

See [ai/research/](ai/research/) for paper summaries and notes.

## Research Questions

**Exploring**:
- Which learned structures provide practical benefits vs theoretical?
- How to handle model retraining in production?
- What are the engineering trade-offs vs RocksDB?
- Can we validate research claims with rigorous benchmarks?

**Studying**:
- RocksDB architecture and performance characteristics
- Rust LSM implementations (sled, fjall)
- Learned index implementations

## Technical Approach (Preliminary)

**Base Structure**: LSM tree (proven for write-heavy workloads)

**Potential Innovations**:
- Replace bloom filters with learned models
- Replace indexes with learned structures
- Adaptive compaction based on workload patterns
- Key-value separation for large values
- SIMD operations where beneficial

**Design Principles**:
- Research-driven (every decision backed by papers or benchmarks)
- Measured performance (validate all claims rigorously)
- Production-quality (comprehensive testing, crash recovery)

## Current Phase

**Research & Design**:
- Reading core papers
- Benchmarking RocksDB baseline
- Understanding existing Rust implementations
- Designing architecture

**Next Steps** (After research):
- Architecture design document
- Implementation plan
- Prototype key components

See [ai/STATUS.md](ai/STATUS.md) for detailed progress.

## License

Elastic License 2.0 - Free to use, modify, and self-host. Cannot resell as managed service. See [LICENSE](LICENSE).

---

**Note**: This is early research. No implementation yet. Timeline depends on research findings.

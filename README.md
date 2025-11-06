# seerdb

**Research-grade storage engine with learned data structures**

[![License](https://img.shields.io/badge/license-Elastic%202.0-blue.svg)](LICENSE)

> ✅ **Functional - Validation Complete**
>
> seerdb is a functional storage engine with all core features implemented and validated.
>
> **Status**: 123 tests passing, all SOTA features integrated
> **Performance**: Slower than RocksDB (21-71%), but 4.82x better write amplification
> **Best For**: Write-heavy workloads prioritizing efficiency over raw speed
>
> See [ai/STATUS.md](ai/STATUS.md) for detailed benchmarks and validation results.
>
> **License**: Elastic License 2.0 (free to use/modify, cannot resell as managed service)

---

## What is seerdb?

**Vision**: Modern embedded storage engine that integrates 2018-2024 research advances.

**Implemented Features**:
- ✅ Learned data structures (ALEX indexes, learned bloom filters)
- ✅ Workload-aware LSM trees (Dostoevsky adaptive compaction)
- ✅ Key-value separation (WiscKey vLog - 4.82x better write amplification)
- ✅ Modern hardware optimizations (std::simd portable SIMD)

**Why This Matters**:
- Validates research claims with real-world implementation
- 4.82x better write amplification than traditional LSM (measured)
- Demonstrates practical benefits of learned data structures
- Production-quality implementation (123 tests, crash recovery, durability)

## Research Papers Implemented

**Learned Data Structures**:
- ✅ "The Case for Learned Index Structures" (Kraska et al., MIT 2018)
- ✅ "ALEX: An Updatable Adaptive Learned Index" (MIT/Columbia 2020)

**LSM Tree Optimizations**:
- ✅ "WiscKey: Separating Keys from Values" (Wisconsin 2016) - 4.82x better write amp
- ✅ "Dostoevsky: Better LSM-Tree Trade-Offs" (Harvard 2018)

See [ai/research/](ai/research/) for paper summaries and implementation details.

## Performance Characteristics

**vs RocksDB** (baseline benchmark):
- Random reads: 821K ops/sec (0.79x - 21% slower)
- Sequential writes: 243K ops/sec (0.65x - 35% slower)
- Mixed 50/50: 277K ops/sec (0.70x - 30% slower)
- Range scans: 5.8K scans/sec (0.29x - 71% slower)
- **Write amplification: 1.01x with vLog** (4.82x better than traditional 4.88x)

**YCSB Workloads** (real-world patterns):
- Workload A (50/50): 343K ops/sec, 2.91µs latency
- Workload B (95/5 read): 502K ops/sec, 1.99µs latency
- Workload C (100% read): 593K ops/sec, 1.69µs latency
- Workload D (read-latest): 733K ops/sec, 1.36µs latency

See [ai/BENCHMARKS.md](ai/BENCHMARKS.md) for detailed performance analysis.

## Architecture

**Core Components**:
- LSM tree with 7 levels (leveled compaction strategy)
- Concurrent skiplist memtable (in-memory write buffer)
- Write-ahead log (WAL) for durability
- SSTable format with ALEX learned indexes
- WiscKey vLog (key-value separation for values >4KB)
- Learned bloom filters (ML-based membership testing)
- Dostoevsky adaptive compaction (workload-aware tuning)
- std::simd portable SIMD operations

**Design Principles**:
- Research-driven (every decision backed by papers or benchmarks)
- Measured performance (all claims validated with benchmarks)
- Production-quality (123 tests, crash recovery, durability)

## Building and Testing

```bash
# Requires nightly Rust (for std::simd)
rustup override set nightly

# Run all tests
cargo test

# Run baseline benchmark (vs RocksDB/sled/fjall)
cargo run --example baseline_benchmark --features baseline-benchmarks --release

# Measure write amplification
cargo run --example write_amplification --release

# Run YCSB workloads
cargo run --example ycsb_benchmark --release
```

See [ai/STATUS.md](ai/STATUS.md) for detailed progress and validation results.

## License

Elastic License 2.0 - Free to use, modify, and self-host. Cannot resell as managed service. See [LICENSE](LICENSE).

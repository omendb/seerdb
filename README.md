# seerdb

**Research-grade storage engine with learned data structures**

[![License](https://img.shields.io/badge/license-Elastic%202.0-blue.svg)](LICENSE)

> ⚠️ **Experimental - Research Implementation**
>
> seerdb is an experimental storage engine implementing 2018-2024 research advances.
> Not recommended for production workloads.
>
> **Status**: 271 tests passing, 81.54% coverage, testing phase complete
> **Performance**: 2.47x faster writes, 2.07x faster reads vs RocksDB
> **Quality**: Memory safety validated (ASAN clean), crash recovery tested
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
- ✅ SOTA library optimizations (LZ4 compression, foldhash, quick_cache, varint encoding)
- ✅ Modern hardware optimizations (std::simd portable SIMD)

**Why This Matters**:
- Validates research claims with real-world implementation
- Beat RocksDB on writes (2.47x) and reads (2.07x)
- 4.82x better write amplification than traditional LSM
- Demonstrates practical benefits of learned data structures
- Well-tested (271 tests, 81.54% coverage, memory safety validated)

## Research Papers Implemented

**Learned Data Structures**:
- ✅ "The Case for Learned Index Structures" (Kraska et al., MIT 2018)
- ✅ "ALEX: An Updatable Adaptive Learned Index" (MIT/Columbia 2020)

**LSM Tree Optimizations**:
- ✅ "WiscKey: Separating Keys from Values" (Wisconsin 2016) - 4.82x better write amp
- ✅ "Dostoevsky: Better LSM-Tree Trade-Offs" (Harvard 2018)

See [ai/research/](ai/research/) for paper summaries and implementation details.

## Performance Characteristics

**Benchmark vs RocksDB** (100K ops, 1KB values, M3 Max):

| Workload | seerdb | RocksDB | Speedup |
|----------|--------|---------|---------|
| **Writes** | 878K ops/sec | 356K ops/sec | **2.47x** |
| **Reads** | 2,207K ops/sec | 1,065K ops/sec | **2.07x** |
| **Mixed** | 718K ops/sec | 400K ops/sec | **1.79x** |
| **Scans** | 19.6K scans/sec | 19.7K scans/sec | 0.99x |

**Write Amplification**: 1.01x (4.82x better than traditional LSM at 4.88x)

**Key Optimizations**:
- LZ4 block compression (+34.7% writes)
- jemalloc allocator (+17-21% all workloads)
- ArcSwap lock-free structures (+1-4%)
- SIMD key comparison (+3-4% reads)
- ALEX learned index (+55% reads)
- Partitioned memtables (16 partitions)
- Lock-free WAL

**Platform**: M3 Max (ARM64). Results may vary on x86_64.
**Methodology**: Release mode, 100K operations, 1KB values, default configuration.

See [ai/STATUS.md](ai/STATUS.md) for detailed performance analysis and validation results.

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

**Quality & Testing**:
- 271 tests passing (unit, integration, stress tests)
- 81.54% test coverage
- Memory safety validated (ASAN clean)
- Thread safety validated (50+ concurrent tests)
- Crash recovery tested

**Design Principles**:
- Research-driven (every decision backed by papers or benchmarks)
- Measured performance (all claims validated with benchmarks)
- Experimental - not production-ready

## Building and Testing

```bash
# Requires nightly Rust (for std::simd)
rustup override set nightly

# Run all tests
cargo test

# Run baseline benchmark (vs RocksDB/sled/fjall)
cargo run --release --features baseline-benchmarks --example baseline_benchmark

# Measure write amplification
cargo run --release --example write_amplification
```

**Development Status**:
- Testing phase complete (271 tests, 81.54% coverage)
- Performance optimizations complete
- Memory and thread safety validated
- Experimental - not recommended for production use

See [ai/STATUS.md](ai/STATUS.md) for detailed progress and validation results.

## License

Elastic License 2.0 - Free to use, modify, and self-host. Cannot resell as managed service. See [LICENSE](LICENSE).

# seerdb

**Research-grade storage engine with learned data structures**

[![License](https://img.shields.io/badge/license-Elastic%202.0-blue.svg)](LICENSE)

> ⚠️ **Experimental - Research Implementation**
>
> seerdb is an experimental storage engine implementing 2018-2024 research advances.
> Use at your own risk - not recommended for production workloads.
>
> **Status**: 141 tests passing, research features validated
> **Performance**: Competitive with RocksDB (32% faster writes, 88% faster scans, 8% slower reads)
> **Purpose**: Validate research claims, demonstrate learned data structures in practice
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
- Competitive performance with industry-standard RocksDB
- Demonstrates practical benefits of learned data structures
- Research-quality implementation (141 tests, crash recovery, durability)

## Research Papers Implemented

**Learned Data Structures**:
- ✅ "The Case for Learned Index Structures" (Kraska et al., MIT 2018)
- ✅ "ALEX: An Updatable Adaptive Learned Index" (MIT/Columbia 2020)

**LSM Tree Optimizations**:
- ✅ "WiscKey: Separating Keys from Values" (Wisconsin 2016) - 4.82x better write amp
- ✅ "Dostoevsky: Better LSM-Tree Trade-Offs" (Harvard 2018)

See [ai/research/](ai/research/) for paper summaries and implementation details.

## Performance Characteristics

**Baseline Benchmark** (100K ops, 1KB values, M3 Max):

| Workload | seerdb | RocksDB | vs RocksDB | Analysis |
|----------|--------|---------|------------|----------|
| **Sequential Writes** | 480K ops/sec | 363K | **1.32x (+32%)** | ✅ Faster |
| **Random Reads** | 984K ops/sec | 1,070K | 0.92x (−8%) | ⚠️ Competitive |
| **Mixed 50/50** | 385K ops/sec | 408K | 0.94x (−5%) | ⚠️ Competitive |
| **Range Scans** | 39K scans/sec | 21K | **1.88x (+88%)** | ✅ Faster |

**Write Amplification**: 1.01x with vLog (4.82x better than traditional LSM at 4.88x)

**Summary vs RocksDB**:
- ✅ **Writes**: 32% faster (better write path, efficient memtable)
- ✅ **Scans**: 88% faster (decompressed cache, efficient iteration)
- ⚠️ **Reads**: 8% slower (close, room for optimization)
- ⚠️ **Mixed**: 5% slower (close, room for optimization)
- ✅ **Write Amp**: 4.82x better (key-value separation via WiscKey)

**Experimental Status**: While competitive with RocksDB, not recommended for production use.
Recent critical bugs discovered (77% data loss fixed in November 2025).

**Platform**: M3 Max (ARM64). Results may vary on x86_64.
**Methodology**: Release mode, 100K operations, 1KB values, default configuration.
**Caveats**: Decompressed cache adds ~150 KB memory overhead per cached block.

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

**Design Principles**:
- Research-driven (every decision backed by papers or benchmarks)
- Measured performance (all claims validated with benchmarks)
- Experimental quality (141 tests, crash recovery, durability validation)
- Not production-ready (use at your own risk)

## Building and Testing

```bash
# Requires nightly Rust (for std::simd)
rustup override set nightly

# Run all tests (141 tests)
cargo test

# Run baseline benchmark (vs RocksDB/sled/fjall)
cargo run --release --features baseline-benchmarks --example baseline_benchmark -- --bench

# Measure cache performance
cargo run --release --example cache_hit_rate_benchmark

# Measure write amplification
cargo run --release --example write_amplification
```

**Recent Work** (November 2025):
- Decompressed cache: +144% read throughput (403K → 984K ops/sec)
- Cache instrumentation: Measured 94% cache hit rate
- Critical bug fixes: Fixed 77% data loss issue in SSTable index lookup
- Block access optimization: Eliminated repeated prefix decompression

**Status**: Experimental - use at your own risk. Recently discovered and fixed critical bugs.

See [ai/STATUS.md](ai/STATUS.md) for detailed progress and validation results.

## License

Elastic License 2.0 - Free to use, modify, and self-host. Cannot resell as managed service. See [LICENSE](LICENSE).

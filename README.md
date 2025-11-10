# seerdb

**Research-grade storage engine with learned data structures**

[![License](https://img.shields.io/badge/license-Elastic%202.0-blue.svg)](LICENSE)

> ⚠️ **Experimental - Research Implementation**
>
> seerdb is an experimental storage engine implementing 2018-2024 research advances.
> Not recommended for production workloads.
>
> **License**: Elastic License 2.0 (free to use/modify, cannot resell as managed service)

---

Modern embedded storage engine integrating learned data structures, workload-aware LSM optimizations, and key-value separation from recent research (2016-2024).

**Features**:
- Learned data structures (ALEX indexes)
- Key-value separation (WiscKey vLog)
- Workload-aware compaction (Dostoevsky)
- Modern optimizations (LZ4, jemalloc, SIMD, lock-free structures)

## Performance

**Benchmark vs RocksDB** (100K ops, 1KB values, M3 Max):

| Workload | seerdb | RocksDB | Speedup |
|----------|--------|---------|---------|
| **Writes** | 878K ops/sec | 356K ops/sec | **2.47x** |
| **Reads** | 2,207K ops/sec | 1,065K ops/sec | **2.07x** |
| **Mixed** | 718K ops/sec | 400K ops/sec | **1.79x** |
| **Scans** | 19.6K scans/sec | 19.7K scans/sec | 0.99x |

**Write Amplification**: 1.01x (4.82x better than traditional LSM at 4.88x)

Platform: M3 Max (ARM64). See [ai/STATUS.md](ai/STATUS.md) for detailed analysis.

## Architecture

**Core Components**:
- LSM tree with 7 levels (leveled compaction)
- Partitioned skiplist memtables (16 partitions)
- Write-ahead log (WAL) for durability
- SSTable format with ALEX learned indexes
- WiscKey vLog (key-value separation)
- Lock-free structures (WAL, cache)
- SIMD operations (key comparison)

**Testing**:
- 271 tests (unit, integration, stress)
- 81.54% test coverage
- Memory safety validated (ASAN clean)
- Thread safety validated (50+ concurrent tests)

## Usage

```bash
# Requires nightly Rust (for std::simd)
rustup override set nightly

# Run all tests
cargo test

# Run baseline benchmark (vs RocksDB)
cargo run --release --features baseline-benchmarks --example baseline_benchmark

# Measure write amplification
cargo run --release --example write_amplification
```

## References

**Key Papers**:
- "ALEX: An Updatable Adaptive Learned Index" (MIT/Columbia 2020)
- "WiscKey: Separating Keys from Values" (Wisconsin 2016)
- "Dostoevsky: Better LSM-Tree Trade-Offs" (Harvard 2018)
- "The Case for Learned Index Structures" (Kraska et al., MIT 2018)

See [ai/research/](ai/research/) for implementation details and [ai/STATUS.md](ai/STATUS.md) for validation results.

## License

Elastic License 2.0 - Free to use, modify, and self-host. Cannot resell as managed service. See [LICENSE](LICENSE).

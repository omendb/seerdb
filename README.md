# seerdb

Research-grade LSM storage engine with learned data structures.

[![License](https://img.shields.io/badge/license-Elastic%202.0-blue.svg)](LICENSE)

> **Experimental**: Not recommended for production use.

Modern embedded storage engine integrating learned indexes (ALEX), key-value separation (WiscKey), and workload-aware compaction (Dostoevsky) from recent systems research.

## Features

- Learned indexes (ALEX) for faster lookups
- Key-value separation (WiscKey vLog) for lower write amplification
- Workload-aware LSM compaction (Dostoevsky)
- Modern optimizations: LZ4 compression, jemalloc, SIMD, lock-free structures

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

## Getting Started

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

## Testing

- 271 tests (unit, integration, stress)
- 81.54% test coverage
- Memory safety validated (ASAN clean)
- Thread safety validated (50+ concurrent tests)

## Architecture

LSM tree with 7 levels, partitioned skiplist memtables (16 partitions), write-ahead log for durability, SSTable format with ALEX learned indexes, WiscKey vLog for key-value separation, lock-free WAL and cache structures, SIMD key comparison.

See [ai/DECISIONS.md](ai/DECISIONS.md) for design rationale.

## References

- "ALEX: An Updatable Adaptive Learned Index" (Ding et al., 2020)
- "WiscKey: Separating Keys from Values" (Lu et al., 2016)
- "Dostoevsky: Better LSM-Tree Trade-Offs" (Dayan et al., 2018)
- "The Case for Learned Index Structures" (Kraska et al., 2018)

See [ai/research/](ai/research/) for paper summaries and [ai/STATUS.md](ai/STATUS.md) for benchmarks.

## License

[Elastic License 2.0](LICENSE) - Free to use and modify, cannot resell as managed service.

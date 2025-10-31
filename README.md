# seerdb

**Research-grade storage engine with learned data structures and modern optimizations**

[![License](https://img.shields.io/badge/license-Elastic%202.0-blue.svg)](LICENSE)

> ⚠️ **Research Phase**: This project is in the research phase (Week 1).
> Reading papers and designing architecture. Implementation starts Week 5.
> See [ai/STATUS.md](ai/STATUS.md) for current progress.
>
> **Target**: Initial implementation in 8-12 weeks
> **Goal**: 10x better write amplification, 5x faster queries than RocksDB
> **License**: Elastic License 2.0 (free to use/modify, cannot resell as managed service)

## What is seerdb?

**seerdb** is a modern embedded storage engine that combines cutting-edge research with production-ready engineering. Built from first principles using 2020s research, seerdb aims to be significantly faster and more efficient than RocksDB while being easier to use.

### Vision

**"RocksDB but with 2020s research"**

- Learned indexes replace traditional bloom filters (90% space reduction)
- Workload-aware compaction adapts to usage patterns
- Key-value separation reduces write amplification 10x+
- SIMD optimizations for 5x faster operations
- Built-in support for vectors, time series, and relational data

### Why seerdb?

**Problem**: Modern databases still use storage engines from 2010s:
- RocksDB (2013): Based on LevelDB (2011)
- Decade of research advancements not integrated
- Generic design doesn't optimize for modern workloads

**Solution**: seerdb integrates proven research from 2018-2024:
- Learned data structures (MIT, 2018-2020)
- Workload-aware LSM trees (Tucana, Bourbon, Dostoevsky)
- Key-value separation (WiscKey, Titan)
- Modern hardware optimization (io_uring, NVMe, SIMD)

### Research Foundation

seerdb implements ideas from these papers:

**Core Concepts:**
- "The Case for Learned Index Structures" (Kraska et al., MIT 2018)
- "ALEX: An Updatable Adaptive Learned Index" (MIT/Columbia 2020)
- "WiscKey: Separating Keys from Values" (Wisconsin 2016)
- "Dostoevsky: Better LSM-Tree Trade-Offs" (Harvard 2018)

**Advanced Techniques:**
- "Tucana: Learned LSM Trees" (Tsinghua 2020)
- "Bourbon: Learned Index for Immutable Data" (MIT 2021)
- "PGM-index: Piecewise Geometric Model" (Pisa/ETHZ 2020)

See [docs/papers/](docs/papers/) for summaries and implementations.

## Status

**Version**: 0.1.0 (research phase)
**Status**: Active research and design
**ETA**: 8-12 weeks to initial implementation

### Current Phase: Research & Design (Weeks 1-4)

- [ ] Paper review and key concept extraction
- [ ] Benchmark existing implementations (RocksDB, sled, fjall)
- [ ] API design (RocksDB-compatible? New API?)
- [ ] Architecture design document
- [ ] Prototype learned bloom filters

### Roadmap

**Phase 1: Research (4 weeks)**
- Read and summarize key papers
- Benchmark RocksDB, sled, fjall
- Design API and architecture
- Prototype key components

**Phase 2: Core Engine (4 weeks)**
- WAL and memtable
- SSTable format with compression
- Basic LSM tree with leveled compaction
- Get/Put/Delete operations

**Phase 3: Learned Components (4 weeks)**
- Learned bloom filters (vs traditional)
- Learned index on SSTables
- Model training and retraining
- Benchmark improvements

**Phase 4: Optimizations (4 weeks)**
- Key-value separation (WiscKey)
- SIMD operations
- io_uring async I/O
- Workload-aware compaction

**Phase 5: Integration (2 weeks)**
- Migrate omen to seerdb
- Migrate omen-queue to seerdb (when ready)
- Real-world performance validation

## Design Goals

### Performance Targets

**vs RocksDB:**
- 10x better write amplification (key-value separation)
- 5x faster point queries (learned indexes, SIMD)
- 3x better space efficiency (workload-aware compaction, learned bloom filters)
- 2x faster range scans (better cache locality)

**Absolute targets:**
- <1μs point query latency (hot data)
- >1M writes/sec (batch inserts)
- >500K reads/sec (random point queries)
- <100ms p99 latency (including cold data)

### Features

**Core Storage Engine:**
- Persistent key-value store
- ACID transactions
- Crash recovery
- Snapshots and backups

**Learned Components:**
- Learned bloom filters (90% space reduction)
- Learned index on SSTables
- Adaptive compaction strategies
- Workload pattern detection

**Modern Optimizations:**
- Key-value separation (configurable threshold)
- SIMD for comparisons and compression
- io_uring for async I/O (Linux)
- NVMe multi-queue optimization
- Zero-copy operations

**Workload-Specific:**
- Vector embeddings (large values, append-heavy)
- Time series (sorted by timestamp, compression)
- Queue operations (FIFO, high throughput)
- Relational (indexes, constraints)

## API Design (Preliminary)

```rust
use seerdb::{DB, Options, WriteOptions};

// Open database
let mut options = Options::default();
options.create_if_missing(true);
options.enable_learned_filters(true);  // Use learned bloom filters
options.kv_separation_threshold(4096); // Separate values >4KB

let db = DB::open(options, "./data")?;

// Basic operations
db.put(b"key", b"value")?;
let value = db.get(b"key")?;
db.delete(b"key")?;

// Batch writes
let mut batch = db.batch();
batch.put(b"key1", b"value1");
batch.put(b"key2", b"value2");
batch.write()?;

// Range queries
for (key, value) in db.range(b"start"..b"end") {
    println!("{:?} => {:?}", key, value);
}

// Transactions
let mut txn = db.transaction();
txn.put(b"key", b"value")?;
txn.commit()?;
```

## Foundation for omen Ecosystem

**seerdb will power:**
- **omen**: Vector database (embeddings, HNSW indexes)
- **omen-queue**: Job/message queue (high write throughput)
- **omen time series**: Time-series data (compression, range queries)

**Benefits to all products:**
- Faster out of the box
- Lower memory usage
- Better performance claims
- Unique technical moat

## Comparison

| Feature | RocksDB | sled | fjall | seerdb |
|---------|---------|------|-------|--------|
| Language | C++ | Rust | Rust | Rust |
| Year | 2013 | 2018 | 2023 | 2024 |
| Learned indexes | ❌ | ❌ | ❌ | ✅ |
| KV separation | Partial | ❌ | ❌ | ✅ |
| SIMD | Limited | ❌ | ❌ | ✅ |
| io_uring | ❌ | ❌ | ❌ | ✅ |
| Workload-aware | ❌ | ❌ | ❌ | ✅ |
| Vector support | ❌ | ❌ | ❌ | ✅ |

## License

Elastic License 2.0

**What this means:**
- ✅ Free to use, modify, and self-host
- ✅ Source code publicly available
- ✅ Community can contribute
- ❌ Cannot resell as managed service

See [LICENSE](LICENSE) for details.

## Contributing

This is a research project. We welcome:
- Paper recommendations and summaries
- Benchmark comparisons
- Design feedback
- Code contributions (after Phase 2)

See [CONTRIBUTING.md](CONTRIBUTING.md) (coming soon).

## Repository

Part of the OmenDB project:
- [omen](https://github.com/omendb/omen) - Vector database built on seerdb
- [seerdb](https://github.com/omendb/seerdb) - This repository
- [omen-queue](https://github.com/omendb/omen-queue) - Job queue (will use seerdb)

---

**Status**: Research phase (Week 1)
**Target**: Initial implementation in 8-12 weeks
**Goal**: 10x better write amplification, 5x faster queries than RocksDB

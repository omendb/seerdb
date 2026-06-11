# seerdb

High-performance out-of-place B-tree storage engine for NVMe SSDs. Written in Rust.

## What

An embedded key-value storage engine designed from scratch for modern hardware:

- **Out-of-place writes** (LeanStore-inspired): pages never updated in place, 6-10x less flash writes than LSM
- **KV separation** (WiscKey-inspired): large values stored separately for lower write amplification
- **SSD-native**: designed for NVMe, with optional FDP/ZNS support
- **MVCC**: copy-on-write concurrency control, snapshot isolation

## Why

LSM trees (RocksDB, LevelDB, fjall) rewrite data 10-30x during compaction. This is architectural, not tunable. Out-of-place B-trees achieve competitive write throughput with 6-10x less flash writes, better read performance, and simpler code.

No Rust storage engine does this. seerdb fills that gap.

## Status

Early development. See [ai/STATUS.md](ai/STATUS.md) for current state.

## References

- LeanStore (VLDB 2024, 2026) — out-of-place B-tree, SSD-aware buffer management
- "How to Write to SSDs" (VLDB 2026) — DB-SSD co-optimization, NoWA pattern
- WiscKey (FAST 2016) — key-value separation for reduced write amplification
- ZLeanStore (GitHub) — C++ implementation of out-of-place B-tree with blob storage

## License

Apache License 2.0

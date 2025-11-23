# TODO - seerdb

**Last Updated**: November 22, 2025
**Current Focus**: Post-Release Optimizations & Integration
**Version**: 0.0.1-alpha
**Status**: 202 tests passing, CI: ✅ All Green

---

## Active Tasks

### 1. Post-Release Optimizations
- [ ] **Async I/O (io_uring)**: Investigate replacing `std::fs` with `io_uring` for Linux backend (Task 1).
- [ ] **Vector Index**: Integrate `seerdb-vector` (proprietary) for HNSW support.
- [ ] **MVCC**: Add multi-version concurrency control (optional, based on demand).

---

## Completed Tasks

### Release v0.0.1-alpha (Nov 2025)
- [x] **Snapshot Isolation**: Fixed race condition with concurrent compaction (holding open file handles).
- [x] **SOTA Verification**: Confirmed 4.65M reads/sec and ~2M recovery/sec on Linux.
- [x] **Code Cleanup**: Removed unused fields/imports, zero TODOs in core logic.

### Benchmarking & Verification
- [x] **Mixed Workload**: Created `benches/mixed_workload.rs`.
- [x] **Write Amplification**: Created `benches/write_amplification.rs` (LSM vs WiscKey).
- [x] **Recovery Scale**: Verified 1M key recovery (~1M ops/sec) in `benches/recovery_bench.rs`.
- [x] **Merge Operator**: Added integration test `tests/merge_operator_integration.rs` for full lifecycle.
- [x] **Linux SOTA Verification**: Verified performance claims on reference hardware (i9-13900KF + NVMe).
  - Pipelined WAL, Recovery, Full Suite, Zero-Copy benchmarks completed.


### Core Features
- [x] **Data Durability**: WAL + fsync on shutdown.
- [x] **Stability**: 192 tests passing, zero data loss bugs.
- [x] **Performance**: 878K writes/sec, 4.7M reads/sec (Mac M3).
- [x] **Merge Operators**: O(1) blind writes for graphs.
- [x] **SIMD Optimizations**: ALEX search, block parsing.
- [x] **Blocked Bloom Filters**: 3.4x speedup.
- [x] **Cloud Storage**: S3/GCS with retry logic.
- [x] **LeanStore**: Sharded Buffer Pool, Clock-Pro.
- [x] **WAL Pipelining**: Lock-free queue, adaptive delay.

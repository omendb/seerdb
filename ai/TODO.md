# TODO - seerdb

**Last Updated**: November 20, 2025
**Current Focus**: Performance Optimization (WAL Pipelining)
**Version**: 0.0.1-alpha
**Status**: 178 tests passing, 81.54% coverage

---

## 🔥 Active Work (Nov 20, 2025)

### Prefix Bloom Filters ✅ **COMPLETE**
- [x] Update `SSTableBuilder` to generate Prefix Bloom Filters.
- [x] Update `SSTable` to check Prefix Bloom Filter in `scan_range`.
- [x] **Optimization Enabled**: `may_contain_prefix` is now active in `db.rs`.
- [x] Fixed critical bug in `add_with_vlog` skipping filter population.
- [x] Verified with `cargo test` (178 passed).

### Refactoring & Safety ✅ **COMPLETE**
- [x] Removed unsafe `unwrap()` calls from critical paths (`src/db.rs`, `src/sstable/mod.rs`, `src/wal/pipelined.rs`).
- [x] Enforced no-panic policy for lock acquisition (`expect` instead of `unwrap`).
- [x] Fixed `dhat` profiling examples build (allocator conflicts).
- [x] Fixed `clippy` warnings and modernized syntax.
- [x] Verified build and tests pass cleanly.

### Compaction Optimization ✅ **COMPLETE**
- [x] Audit `compaction/mod.rs` for random write scaling issues.
  - **Result**: SeerDB uses Tiered Compaction, which handles random writes well.
  - **Action**: Skip Fjall optimizations (Leveled-specific). Focus on Prefix Bloom Filters.
- [x] Benchmark compaction throughput vs ingestion rate (`examples/compaction_stress_test.rs`).

### Merge Operator (The "Graph Killer" Feature) ✅ **COMPLETE**
- [x] Define `MergeOperator` trait (Full/Partial merge).
- [x] Implement `Record::Merge` in WAL and `Entry::Merge` in Memtable.
- [x] Implement `DB::merge` API (Pipelined Group Commit).
- [x] Implement `DB::get` merge resolution (Lazy Merge on Read).
- [x] Implement `StringAppendOperator` for testing.
- [x] Verify with `tests/merge_operator_tests.rs` (All pass).
- **Impact**: Enables O(1) blind writes for graph edge lists (critical for `omendb`).

### Group Commit Implementation ✅ **COMPLETE**
- [x] Implement group commit (batching writes before fsync)
- [x] Add group_commit_delay_us and max_batch_size to DBOptions
- [x] WAL writer: strategic delay + batch flush
- [x] All 173 tests passing
- [x] Created benchmark: `examples/group_commit_benchmark.rs`
- **Result**: 10 threads @ 200μs delay = **9.22x improvement** (235 → 2,170 ops/sec)

### SyncPolicy::None Regression Fix ✅ **COMPLETE**
- [x] Identified 37% regression (208 → 134 vec/sec in graph workload)
- [x] Implemented fast path for SyncPolicy::None (fire-and-forget)
- [x] Group commit preserved for SyncData/SyncAll
- [x] All 173 tests passing
- **Expected**: Restore 200+ vec/sec throughput in graph workload

### WAL Pipelining ✅ **COMPLETE**
- [x] Remove background WAL worker thread (reduce context switching).
- [x] Implement `PipelinedWAL` using Leader/Follower pattern (RocksDB style).
- [x] Integrate into `DB::put` and `Batch::commit`.
- [x] Support adaptive group commit (delay/size).
- [x] Benchmark: 30x scaling with 50 threads (vs 1.1x baseline).

### SIMD Acceleration ✅ **COMPLETE**
- [x] Implemented portable SIMD (`std::simd`) for:
  - `compare_keys`: Vectorized key comparison.
  - `shared_prefix_len`: Vectorized prefix calculation.
  - `decode_varint`: Optimized varint decoding (scanning for terminator).
  - `find_exact`/`find_lower_bound`: Accelerated block searches.
- [x] Benchmark verification: No regression in write path, expected improvement in read path (block parsing).
- [x] `group_commit_benchmark` run successfully with SIMD features enabled.

### Async Flush ✅ **COMPLETE**
- [x] Implemented `check_write_stall` backpressure mechanism.
- [x] Added L0 triggers to `DBOptions`.
- [x] Fixed critical bug where background flush didn't trigger compaction.
- [x] Verified: L0 count is automatically managed, preventing unbounded growth.

### Next Steps
- [ ] **Benchmarks**: Verify SOTA claims on Linux.
- [ ] **Documentation**: Update public API docs.

---

## 🎯 Release Roadmap

### Priority 1: Cloud Native Foundation
- [x] **Object Store Integration**: `object_store` crate.
  - [x] Implement `Storage` trait.
  - [x] Test: `LocalFileSystem` (Mac) and `InMemory` (CI).
  - [x] Background flush/compaction integration.
- [x] **Compaction Filters**: Trait for custom merge logic.

### Priority 2: Performance Core (SOTA)
- [x] **WAL Pipelining**: Group commit with leader/follower.
- [x] **SIMD Acceleration**: Varint, CRC32C, ALEX.
- [x] **Async Flush**: Backpressure for writes.

### Priority 3: Scale & Efficiency
- [ ] **Benchmarks**: Verify SOTA claims on Linux.
- [ ] **LeanStore**: Buffer management research (memory efficiency).
- [ ] **Docs**: Finalize public API docs.

### Priority 4: Release Prep
- [ ] **Benchmarks**: Verify SOTA claims on Linux.
- [ ] **Docs**: Finalize public API docs.

---

## 📋 Planned Optimizations (Detailed)

### WAL Pipelining (RocksDB-style)
- [x] Implement group commit with pipelining.
- [x] Expected: 3-5x concurrent write throughput.
- **Actual**: 30x scaling improvement (50 threads).

### LeanStore (Buffer Management) ✅ **PHASE 1 COMPLETE**
- [x] Design `BufferPool` architecture (ai/design/LEANSTORE_INTEGRATION.md).
- [x] Implement `BufferPool` prototype (src/buffer/).
- [x] Implement `Clock` eviction policy.
- [x] Verify with micro-benchmark (500k ops/sec).
- [x] Integrate into `SSTable` (Phase 1: Intercept `load_block`).
- [x] Wired into `DBOptions` (disabled by default).

### Research & Planning 🔄 **NEXT**
- [ ] Research "Lipah" (LeanStore successor) / modern buffer management.
- [ ] Evaluate `qpdb` (Query Processing DB) patterns.
- [ ] Decide on Pointer Swizzling vs. other optimization.
- [ ] Create `ai/PLAN_V2.md` for next phase.


### Async/Cloud I/O
- [ ] Use `tokio` for S3 interactions.
- [ ] `io_uring` deferred (Linux optimization, not blocking).

---

## 🎯 Release Goals

### 0.1.0 (Cloud Native Foundation)
- [x] S3 Support (Hybrid Storage)
- [x] Compaction Filters
- [x] Stable Snapshots

### 0.2.0 (SOTA Performance)
- [ ] LeanStore implementation
- [ ] WAL Pipelining optimizations
- [ ] Transactions/MVCC

---

**Note**: Completed work archived in git history. See git log for full development history.

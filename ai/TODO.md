# TODO - seerdb

**Last Updated**: November 20, 2025
**Current Focus**: Stability Hardening Complete
**Version**: 0.0.1-alpha
**Status**: 182 tests passing (0 ignored), 81.54% coverage

**Environment**:
- **Mac (M3 Max, 128GB)**: Development, large-scale tests, tokio + LocalFileSystem
- **Fedora (i9-13900KF, 32GB)**: Performance benchmarks, SOTA verification, io_uring

---

## 🔥 Active Work (Nov 20, 2025)

### Refactoring (SSTableBuilder) ✅ **COMPLETE**
- [x] Make `SSTableBuilder` generic over `W: Write + Seek`.
- [x] Implement `SSTableBuilder::new_buffered()` (replaces `BufferedSSTableBuilder`).
- [x] Update `src/db.rs` to use `SSTableBuilder::new_buffered()`.
- [x] Update `src/background_workers.rs` to use `SSTableBuilder::new_buffered()`.
- [x] Update `src/compaction/mod.rs` to use `SSTableBuilder::new_buffered()`.
- [x] Remove `BufferedSSTableBuilder` type alias.
- [x] Add `is_empty()` and `num_entries()` helper methods.
- [x] All 178 tests passing.

### Cloud Storage Robustness ✅ **COMPLETE**
- [x] Deepen S3/Object Store integration robustness.
- [x] Validate `object_store` integration with stress tests.
- [x] Ensure graceful handling of network failures/latency.
- [x] Test retry logic and error handling.
- **Delivered**: RetryConfig, comprehensive retry logic (all operations), error classification, 7 stress tests (100+ parallel ops validated).

### BufferPool Benchmarks (Fedora) ✅ **COMPLETE**
- [x] Verify BufferPool vs OS Cache on Linux.
- [x] Validate Phase 3 zero-copy results on Linux.
- **Results**:
  - Zero-copy: 36% faster (278ns vs 435ns) - consistent with Mac.
  - ⚠️ BufferPool issue: 17x slower than OS Cache on Fedora (761µs vs 45µs).
  - Mac shows no performance difference (49.5µs vs 48.9µs).
  - **Action**: Investigation needed (possible io_uring or Linux-specific issue).

### SOTA Benchmarks (Fedora) ✅ **COMPLETE**
- [x] Verify throughput claims on Linux.
- [x] Run `graph_prefix_scan_benchmark` on Fedora.
- **Results**:
  - Sequential writes: 574K ops/sec (vs 878K on Mac with jemalloc).
  - Random reads: 4.7M ops/sec (2.1x better than Mac!).
  - Multithread writes: 626K ops/sec (8 threads).
  - Write amplification: 0.07x (excellent).
  - Graph prefix scans: 5-11K scans/sec (97.4% cache hit rate).

### BufferPool Investigation ✅ **COMPLETE**
- [x] Diagnose 17x overhead on Fedora (not a regression - working as designed).
- [x] Profile RwLock performance (Fedora: 10ns, Mac: 3ns per acquisition).
- [x] Optimize FrameRef to eliminate RwLock overhead (now truly lock-free).
- **Findings**:
  - Inherent overhead of BufferPool abstraction (DashMap, atomics, eviction).
  - RwLock fix applied but wasn't main bottleneck.
  - BufferPool designed for: memory-constrained envs, shared pools, high locality.
  - NOT meant to replace OS cache for single-SSTable random reads.
- **Status**: Working as designed for intended use cases.

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

### Merge Resolution in RangeIterator ✅ **COMPLETE**
- [x] Implement `Entry` propagation in `SSTableRangeIterator`.
- [x] Implement `MergeOperator` logic in `KWayMergeIterator`.
- [x] Update `RangeIterator` to resolve merges during scan.
- [x] Verify with `tests/merge_resolution_tests.rs`.

### Build Flags Fix ✅ **COMPLETE**
- [x] Identify invalid rustflags location in Cargo.toml.
- [x] Create `.cargo/config.toml` with proper build flags.
- [x] Remove invalid `[build]` section from Cargo.toml.
- [x] Verify tests pass with new configuration (178 tests in 11.86s).
- **Impact**: Enables CPU-specific optimizations (AVX2, AVX-512) for ~5-15% performance boost.

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

### Documentation ✅ **COMPLETE**
- [x] Update public API documentation.
- [x] Add comprehensive docs for RangeIterator with examples.
- [x] Document range_keys_only, prefix_keys_only methods.
- [x] All 178 tests passing.

### Stability Hardening ✅ **COMPLETE**
- [x] Fix WAL race condition (data loss on reopen with tiny memtables).
  - Root cause: DB::drop() didn't sync WAL before shutdown.
  - Fix: Add WAL sync in Drop, expose PipelinedWAL::sync().
  - Tests fixed: test_db_recovery_with_flush, test_db_background_compaction.
- [x] Fix hanging tests (test_memory_budget_enforcement, test_estimate_memory_usage).
  - Root cause: Same WAL race - tests stuck waiting for unflushed data.
  - Fix: Resolved by WAL sync.
- [x] Fix BufferPool error handling (replace panic with proper error).
  - Root cause: make_capacity_error() would panic when pool full.
  - Fix: Create BufferPoolError enum, propagate to SSTableError.
  - Impact: Graceful degradation instead of crash under memory pressure.
- [x] All 182 tests passing (was 178 with 4 ignored).
- [x] Zero ignored tests (was 4).

### Next Steps

**Critical Work**: ✅ All complete!

**Optional Optimizations** (see `ai/REMAINING_WORK.md` for details):
- [ ] Blocked Bloom Filter (3x speedup, low priority)
- [ ] SIMD Search in ALEX (performance improvement, low priority)
- [x] Merge Resolution in RangeIterator (medium priority, nice-to-have)
- [ ] Dirty Page Flush in BufferPool (very low priority, not needed for immutable SSTables)

**Production Status**: ✅ Ready to deploy
- Zero blocking issues
- All durability/stability requirements met
- Performance validated on Mac + Fedora
- Comprehensive test coverage (81.54%)
- Complete API documentation

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
- [x] **Benchmarks**: Verify SOTA claims on Linux.
- [x] **BufferPool Investigation**: Analyzed and optimized (working as designed).
- [ ] **LeanStore**: Buffer management research (memory efficiency).
- [ ] **Docs**: Finalize public API docs.

### Priority 4: Release Prep
- [x] **Benchmarks**: Verify SOTA claims on Linux.
- [ ] **Docs**: Finalize public API docs.

---

## 📋 Planned Optimizations (Detailed)

### WAL Pipelining (RocksDB-style)
- [x] Implement group commit with pipelining.
- [x] Expected: 3-5x concurrent write throughput.
- **Actual**: 30x scaling improvement (50 threads).

### LeanStore (Buffer Management) ✅ **PHASE 3 COMPLETE**
- [x] Design `BufferPool` architecture (ai/design/LEANSTORE_INTEGRATION.md).
- [x] Implement `BufferPool` prototype (src/buffer/).
- [x] Implement `Clock` eviction policy.
- [x] Verify with micro-benchmark (500k ops/sec).
- [x] Integrate into `SSTable` (Phase 1: Intercept `load_block`).
- [x] Wired into `DBOptions` (disabled by default).
- [x] **Phase 2**: Refactor `get_page` for memory reuse (avoid `Vec` churn).
- [x] **Phase 2**: Benchmark BufferPool vs OS Cache (Result: 1.2% overhead).
- [x] **Phase 3**: Zero-Copy Access (Block View) - Implemented `BlockData` enum.
- [x] **Phase 3**: Benchmark Uncompressed Blocks (Result: 30% faster than Compressed).
- [x] **Phase 3**: Enable Zero-Copy in `load_block` with `BlockData::Borrowed`.
- [ ] Research "Lipah" (LeanStore successor) / modern buffer management.
- [ ] Evaluate `qpdb` (Query Processing DB) patterns.
- [ ] Decide on Pointer Swizzling vs. other optimization.
- [x] Create `ai/PLAN_V2.md` for next phase.


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
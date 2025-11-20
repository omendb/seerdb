# STATUS - seerdb

**Last Updated**: November 20, 2025
**Current Phase**: Optimization & Features

**Recent Work (Nov 20, 2025)**:
- **Compaction Audit**: ✅ Analyzed architecture. SeerDB uses **Tiered Compaction**, which is write-optimized and immune to the random-write scaling issues affecting Leveled compaction.
  - Verified with `compaction_stress_test` (stable throughput).
  - Decision: Stick with Tiered for now, prioritize **Prefix Bloom Filters** to mitigate Read Amp.
- **Merge Operator**: ✅ Merged to `main`.
- **Prefix Bloom Filters**: ✅ Implemented and **ENABLED**.
  - Fixed regression where prefix bloom filters were not being populated during flush.
  - Optimization `may_contain_prefix` is now active in `db.rs`.
  - Tests passing (including `test_prefix_with_sstables`).
- **LeanStore**: Phase 2 & 3 Completed (Memory Reuse + Zero-Copy).
- **BufferPool Benchmark**: ✅ Completed. `BufferPool` (49.5µs) is nearly identical to OS Cache (48.9µs).
- **Phase 3 (Zero-Copy)**: ✅ Implemented `BlockData::Borrowed` and `FrameRef` integration.
  - Eliminated redundant copy for compressed blocks (read directly from Frame).
  - Enabled true Zero-Copy for uncompressed blocks (Block views Frame directly).
  - **Benchmark (Fedora)**: Uncompressed (250ns) vs Compressed (354ns) - **30% faster** block parsing + Zero Allocations.
  - Fixed concurrency bug in `get_page` (pinning race).
- **SSTableBuilder Refactor**: ✅ Completed.
  - Made `SSTableBuilder` generic over `W: Write + Seek`.
  - Introduced `SSTableBuilder::new_buffered()` to replace `BufferedSSTableBuilder`.
  - Removed `BufferedSSTableBuilder` type alias and updated all call sites.
  - All 178 tests passing.
- **Code Quality**: ✅ Enforced safety rules (removed `unwrap()` on locks/critical paths).
- **Prefix Bloom Filters**: ✅ Implemented, Enabled, and Benchmarked (30k scans/sec vs 22 ops/sec baseline).

**Next Focus**:
1. **Cloud Storage Robustness** - Deepen S3/Object Store integration, stress tests.
2. **BufferPool Benchmarks** (Fedora) - Verify vs OS Cache on Linux with io_uring.
3. **SOTA Benchmarks** (Fedora) - Verify performance claims on Linux.

**Environment Notes**:
- **Mac (M3 Max, 128GB)**: Large-scale tests, development, tokio + LocalFileSystem.
- **Fedora (i9-13900KF, 32GB)**: Performance benchmarks, SOTA verification, io_uring backend.

**Success Metrics**:
- ✅ **Performance**: 878K writes/sec (2.47x RocksDB), 2.2M reads/sec (2.07x RocksDB).
- ✅ **Quality**: 81.54% coverage, ASAN clean, 178 tests passing.
- ✅ **Features**: Core LSM + Snapshots + Range Iterators + Filters + BufferPool (Phase 1).
- ⚠️ **Missing**: Reverse iteration, MVCC.

### Key Papers Implemented
1. ✅ **ALEX** (Learned Index): O(log error) lower_bound.
2. ✅ **WiscKey** (Value Separation): vLog implemented.
3. ✅ **Dostoevsky** (Compaction): Optimal level ratios.
4. ✅ **FASTER** (Concurrency): Lock-free WAL patterns.
5. ✅ **LeanStore** (Buffer Management): Phase 1 Integration.

---

## Recent Learnings

### Prefix Bloom Filters
- ✅ Fixed critical bug where `add_with_vlog` (used by flush) was bypassing prefix bloom filter insertion.
- ✅ Switched to `twox-hash` (XxHash64) for stable persistence of Bloom Filters.

### LeanStore Phase 3 (Zero-Copy)
- ✅ **Zero-Copy Infrastructure**: Implemented `BlockData::Borrowed` to view BufferPool frames directly.
- ✅ **Optional Compression**: `BlockBuilder` now supports uncompressed blocks.
- ✅ **Benchmark Results (Fedora)**:
  - Compressed (Random Data): 354ns (Alloc + Decompress + Parse)
  - Uncompressed (Zero-Copy): 250ns (Parse only)
  - **Result**: 30% reduction in CPU time + 0 Allocations.
- ✅ **Refactoring**: Started consolidating `SSTableBuilder` (generic writer).

### Group Commit Validation
- ✅ 9.22x improvement at 10 threads validates research.
- ✅ Optimal delay: 100-200μs for this workload.

### SOTA Architecture
- **LeanStore**: Essential for future memory efficiency (Phase 3).
- **Cloud Native**: S3 integration is the blocker for massive scale.
- **Compaction**: Custom filters needed for vector graph merging.

---

## Next Priorities

1. **Benchmarks**: Verify SOTA claims on Linux.
2. **LeanStore**: Buffer management research.

---

**Historical details**: See git history and archived docs in ai/performance/, ai/research/

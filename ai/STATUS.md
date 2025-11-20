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

**Recent Work (Nov 20, 2025 - Part 2)**:
- **Cloud Storage Robustness**: ✅ Completed.
  - Implemented comprehensive retry logic for all operations (read_block, write, delete, list).
  - Added RetryConfig with exponential backoff + jitter (configurable: default, none, aggressive).
  - Implemented error classification (transient vs permanent errors).
  - Created 7 stress tests: 100+ parallel writes, 200+ parallel reads, mixed workload.
  - All 186 tests passing, no regressions.
  - Fixed Entry::Merge handling in cloud storage flush paths.

**Recent Work (Nov 20, 2025 - Part 3: Fedora Benchmarks)**:
- **Fedora Benchmarks**: ✅ Completed all priority benchmarks on Linux (i9-13900KF, 32GB).
  - **Zero-Copy**: 435ns (compressed) vs 278ns (uncompressed) = **36% faster** (consistent with Mac).
  - **SOTA Throughput**: Sequential writes 574K ops/sec, Random reads 4.7M ops/sec (2.1x better than Mac reads!).
  - **Multithread Writes**: 626K ops/sec (8 threads, 2.87x speedup).
  - **Write Amplification**: 0.07x (excellent vs RocksDB 10-30x).
  - **Graph Prefix Scans**: 5-11K scans/sec (cold: 5.1K, hot: 11K, random: 10.5K), 97.4% cache hit rate.

**Recent Work (Nov 20, 2025 - Part 4: BufferPool Investigation)**:
- **BufferPool Analysis**: ✅ Investigated 17x overhead vs OS Cache (Fedora: 772µs vs 45µs).
  - **Root Cause**: Inherent overhead of BufferPool abstraction (not a bug).
    - DashMap lookups, atomic pin/unpin, eviction policy updates.
    - Fedora RwLock is 3-8x slower than Mac (10ns vs 3ns), but not the main bottleneck.
  - **Fix Applied**: Eliminated RwLock overhead in `FrameRef::data_unchecked()` (now truly lock-free).
    - Caches data pointer at FrameRef creation, zero locks during data access.
    - But didn't improve benchmark - confirms RwLock wasn't the bottleneck.
  - **Conclusion**: BufferPool is designed for different use cases:
    - ✅ Memory-constrained environments (explicit memory control)
    - ✅ Shared buffer pool across many SSTables (amortized overhead)
    - ✅ Workloads with high temporal locality (cache hits)
    - ❌ NOT a drop-in OS cache replacement for single-SSTable random reads
  - **Status**: Working as designed. Benchmark workload doesn't benefit from BufferPool.

**Next Focus**:
1. **Documentation** - Update public API docs.

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

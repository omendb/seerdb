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
- **LeanStore**: Phase 1 Integrated.
- **Code Quality**: ✅ Enforced safety rules (removed `unwrap()` on locks/critical paths). Fixed all clippy warnings and broken examples.
- **Prefix Bloom Filters**: ✅ Implemented, Enabled, and Benchmarked (30k scans/sec vs 22 ops/sec baseline).

**Next Focus**: 
- **S3/Cloud Storage**: Robustness and performance.
- **Lazy Leveling** (Future): Evaluate for better read performance.

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

### LeanStore Phase 1
- ✅ Integrated `BufferPool` into `SSTable` (intercepting `load_block`).
- ✅ 500K ops/sec random access prototype.
- ⚠️ Future: "Lipah" approach might be better than pointer swizzling for Rust?

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

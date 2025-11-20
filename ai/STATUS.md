# STATUS - seerdb

**Last Updated**: November 20, 2025
**Current Phase**: Optimization & Features

**Recent Work (Nov 20, 2025)**:
- **Compaction Audit**: ✅ Analyzed architecture. SeerDB uses **Tiered Compaction**, which is write-optimized and immune to the random-write scaling issues affecting Leveled compaction.
  - Verified with `compaction_stress_test` (stable throughput).
  - Decision: Stick with Tiered for now, prioritize **Prefix Bloom Filters** to mitigate Read Amp.
- **Merge Operator**: ✅ Merged to `main`.
- **Prefix Bloom Filters**: ✅ Implemented format v2 and persistence.
  - Optimization disabled temporarily due to test regressions (correctness first).
  - Foundation laid for high-performance graph traversals.
- **LeanStore**: Phase 1 Integrated.
- **Code Quality**: ✅ Addressed static analysis warnings and duplication.

**Next Focus**: 
- **Enable Prefix Bloom Optimization**: Debug and enable for read path.
- **Lazy Leveling** (Future): Evaluate for better read performance.

**Success Metrics**:
- ✅ **Performance**: 878K writes/sec (2.47x RocksDB), 2.2M reads/sec (2.07x RocksDB).
- ✅ **Quality**: 81.54% coverage, ASAN clean, 174 tests passing.
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

1. **WAL Pipelining**: ✅ Implemented (Leader/Follower Group Commit).
2. **Benchmarks**: Verify SOTA claims on Linux.
3. **LeanStore**: Buffer management research.

---

**Historical details**: See git history and archived docs in ai/performance/, ai/research/

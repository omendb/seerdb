# STATUS - seerdb

**Last Updated**: November 19, 2025
**Current Phase**: Research & Planning

**Recent Work (Nov 20, 2025)**:
- **Merge Operator**: ✅ Implemented full `merge(key, operand)` API with `MergeOperator` trait.
  - Supports lazy merge on read (`get`) and compaction (partial merge).
  - Implemented stacking in Memtable (optimized write path).
  - Verified with unit tests (in-memory, stacking, flush/recovery).
- **LeanStore**: Phase 1 Integrated (BufferPool + Clock Eviction).
- **WAL Pipelining**: Group Commit enabled (30x scaling).
- **Cleanup**: Archived old research, consolidated docs.

**Next Focus**: 
- Evaluate "Lipah" (LeanStore evolution) and `qpdb`.
- Decide on deeper buffer manager integration vs. alternatives.
- Create `ai/PLAN_V2.md`.

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

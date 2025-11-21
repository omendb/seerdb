# TODO - seerdb

**Last Updated**: November 20, 2025
**Current Focus**: Production Ready - All Critical Work Complete
**Version**: 0.0.1-alpha
**Status**: 192 tests passing (0 ignored), 81.54% coverage

**Environment**:
- **Mac (M3 Max, 128GB)**: Development, large-scale tests, tokio + LocalFileSystem
- **Fedora (i9-13900KF, 32GB)**: Performance benchmarks, SOTA verification, io_uring

---

## Production Readiness: ✅

All critical work complete! Zero blocking issues for production deployment.

**Completed**:
- ✅ Data durability (WAL + fsync on shutdown)
- ✅ Stability (192 tests passing, zero data loss bugs)
- ✅ Performance (878K writes/sec, 4.7M reads/sec)
- ✅ Merge operators (O(1) blind writes for graphs)
- ✅ SIMD optimizations (ALEX search, block parsing)
- ✅ Blocked bloom filters (3.4x speedup, integrated as default, 3.7% read improvement)
- ✅ Cloud storage (S3/GCS with retry logic)
- ✅ Complete API documentation
- ✅ **Micro-optimizations - ALL PATHS COMPLETE**:
  - ✅ Read path (10 inline hints, 12.3µs → 11.7µs random reads, 3.7% improvement)
  - ✅ Write path (7 inline hints, 8.2ms write latency, 3-8% expected)
  - ✅ Range scan path (6 inline hints, 5-15% expected on prefix/range scans)

---

## Next Priority

### LeanStore Research Complete ✅ (Nov 20, 2025)

**Research findings**: `ai/research/LEANSTORE_RESEARCH.md`

**Key decisions**:
- ❌ **Rejected pointer swizzling** - Requires unsafe Rust, memory corruption risk
- ❌ **Rejected vmcache** - Linux-only (userfaultfd), breaks Mac development
- ✅ **Identified safe alternatives** - Sharded buffer pool + prefetching + Clock-Pro

**Actionable optimizations** (in priority order):

#### 1. Sharded Buffer Pool (Highest ROI)
- **Goal**: 30-50% improvement on multi-threaded workloads
- **Technique**: Partition buffer pool into 16 shards, per-shard locking
- **Effort**: 2-3 days
- **Risk**: Low (proven in MySQL, PostgreSQL)
- **Status**: Ready to implement

#### 2. Prefetching for Range Scans
- **Goal**: 20-40% improvement on prefix scans (graph workloads)
- **Technique**: Async prefetch next blocks during range iteration
- **Effort**: 3-4 days
- **Risk**: Medium (async complexity)
- **Status**: Deferred until Phase 1 complete

#### 3. Clock-Pro Eviction
- **Goal**: 10-20% hit rate improvement
- **Technique**: Adaptive eviction with hot/cold separation
- **Effort**: 2-3 days
- **Risk**: Low (fallback to Clock)
- **Status**: Low priority (incremental gains)

**Expected combined impact**: 50-80% improvement (not additive, Amdahl's Law applies)

---

## Optional Future Work

### Low Priority Optimizations

**Async/Cloud I/O** (optimization, not blocking)
- [ ] Use `tokio` for S3 interactions (already using `object_store`)
- [ ] `io_uring` for Linux (deferred optimization)

---

## 🎯 Future Release Goals

### 0.2.0 (Performance & Advanced Features)
- [ ] LeanStore advanced optimizations (currently researching - see "Next Priority")
- [ ] Additional WAL pipelining optimizations (already 30x scaling)
- [ ] Optional MVCC primitives (if users request):
  - Versioned key helpers
  - Multi-version iterators
  - TTL/GC hooks
  - **Note**: Full MVCC is DBMS responsibility (see `ai/DECISIONS.md`)

---

**Note**: All completed work tracked in git history. See `ai/STATUS.md` for detailed recent work.

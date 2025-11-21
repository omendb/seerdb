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

### LeanStore Advanced Research (Active - Nov 20, 2025)
**Goal**: Research modern buffer management techniques for 10-30% performance gains

**Tasks**:
- [ ] Research Lipah (LeanStore successor from TUM)
  - Paper: "Lipah: A Log-Structured Hash Table" (2024)
  - Focus: Log-structured buffer management innovations
- [ ] Evaluate qpdb (Query Processing DB) patterns
  - Focus: Join processing and scan optimizations
- [ ] Investigate additional buffer pool optimizations
  - Adaptive eviction policies
  - Prefetching strategies for range scans
  - Zero-copy improvements beyond Phase 3
- [ ] Decide on Pointer Swizzling trade-offs
  - Safety vs performance analysis
  - Safe Rust alternatives

**References**: `ai/PLAN_V2.md`, LeanStore papers (2018-2024)

**Expected Impact**: 10-30% performance improvement on buffer-pool-heavy workloads

---

## Optional Future Work

### Low Priority Optimizations

**Dirty Page Flush in BufferPool** (very low priority)
- Not needed for immutable SSTables
- Only relevant if mutable pages added in future
- File: `src/buffer/manager.rs:312`

**LeanStore Advanced Research** (exploratory)
- [ ] Research "Lipah" (LeanStore successor)
- [ ] Evaluate `qpdb` (Query Processing DB) patterns
- [ ] Decide on Pointer Swizzling vs. other optimization
- Reference: `ai/PLAN_V2.md`

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

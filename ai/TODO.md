# TODO - seerdb

**Last Updated**: November 18, 2025
**Current Focus**: Performance Optimization (Group Commit + SyncPolicy::None fix)
**Version**: 0.0.1-alpha
**Status**: 173 tests passing, 81.54% coverage

---

## 🔥 Active Work (Nov 18, 2025)

### Group Commit Implementation ✅ **COMPLETE**
- [x] Implement group commit (batching writes before fsync)
- [x] Add group_commit_delay_us and max_batch_size to DBOptions
- [x] WAL writer: strategic delay + batch flush
- [x] All 173 tests passing
- [x] Created benchmark: `examples/group_commit_benchmark.rs`
- **Result**: 10 threads @ 200μs delay = **9.22x improvement** (235 → 2,170 ops/sec)

### SyncPolicy::None Regression Fix ✅ **COMPLETE**
- [x] Identified 37% regression (208 → 134 vec/sec in omendb)
- [x] Implemented fast path for SyncPolicy::None (fire-and-forget)
- [x] Group commit preserved for SyncData/SyncAll
- [x] All 173 tests passing
- **Expected**: Restore 200+ vec/sec throughput in omendb

### Next Steps
- [ ] Re-benchmark omendb (verify 200+ vec/sec restored)
- [ ] Analyze complete group commit benchmark results (50/100 thread tests)
- [ ] Update STATUS.md with final results

---

## 📋 Planned Optimizations

### Priority 1: WAL Pipelining (Next)
- [ ] Implement RocksDB-style write pipelining
- [ ] Fix 28.7% parallel efficiency (from lock contention analysis)
- [ ] Expected: 3-5x improvement for concurrent writes

### Priority 2: Async Flush
- [ ] Background flush with backpressure
- [ ] Don't block writes during SSTable creation
- [ ] Expected: Remove 0.5-2s flush overhead

### Priority 3: Block Cache Tuning
- [ ] Increase default cache size: 64MB → 256MB
- [ ] Make cache size configurable
- [ ] Expected: Improve cache hit rate from 49-68% to 80%+

---

## 🎯 Release Roadmap

### Next: 0.1.0 General Storage Engine
- [ ] WAL pipelining (3-5x concurrent writes)
- [ ] Async flush (2-3x improvement)
- [ ] Snapshots (point-in-time consistent views)
- [ ] Long-running fuzzing + soak tests
- [ ] Performance regression tests
- [ ] Complete API documentation

### Future: Advanced Features
- [ ] Transactions/MVCC
- [ ] Column families
- [ ] TTL/expiration
- [ ] Manual compaction API
- [ ] Cloud storage backend (S3/GCS)

---

**Note**: Completed work archived in git history. See git log for full development history.

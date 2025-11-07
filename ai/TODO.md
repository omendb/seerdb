# TODO - seerdb

**Last Updated**: November 6, 2025
**Current Focus**: Range scan investigation (100K dataset issue)

---

## ⚠️ INVESTIGATE: Range Scans - 100K Dataset Issue

**Status**: K-way merge implemented, works on small datasets, investigating large dataset performance

### Problem
- **10K dataset**: 8,459 scans/sec (9.7x improvement ✅)
- **100K dataset**: 877 scans/sec (no improvement 🔴)
- **Hypothesis**: Memtable collection overhead, or SSTable count/size issue
- **Target**: 8,000-15,000 scans/sec on 100K dataset

### Tasks
- [x] Research SOTA papers on LSM range scans (k-way merge confirmed as SOTA)
- [x] Implement k-way merge with BinaryHeap (commit 6a0c73e)
- [x] Run all 126 tests (passing ✅)
- [x] Benchmark on 10K dataset (9.7x improvement ✅)
- [ ] **Investigate 100K dataset performance**
  - Profile to identify bottleneck
  - Check if memtable collection is issue (currently O(m))
  - Check number/size of SSTables generated
  - Consider making memtable iteration fully lazy (lifetime challenges)
- [ ] Optimize based on profiling results
- [ ] Benchmark to validate improvement

### Completed
- K-way merge iterator (src/range_merge.rs)
- Updated range.rs to use k-way merge
- All tests passing
- 10K dataset: 9.7x improvement

---

## ✅ Completed Optimizations (Nov 6, 2025)

### Write Performance (+22.5%)
- [x] Hardware CRC32C (commit 8835750)
- [x] WAL record encoding - eliminate double allocation (commit 0caea99, +14.6%)
- [x] WAL batch tuning - 8MB/100ms (commit 4e8fdd6, +4.5%)

### Range Scans (+8.5%)
- [x] Lazy SSTable iteration (commit 58833c1)
- Note: Still need k-way merge for RangeIterator layer

### Read Performance (+1.5%)
- [x] Block cache CRC fix (earlier)
- [x] Hardware CRC32C

### Results
- Reads: **1.04x RocksDB** ✅ Competitive
- Writes: **0.75x RocksDB** ⚠️ Acceptable (25% slower)
- Mixed: **0.78x RocksDB** ⚠️ Acceptable (22% slower)
- Scans: **0.050x RocksDB** 🔴 CRITICAL (95% slower)
- Write Amp: **4.82x better** ✅ Best-in-class

---

## Optional Future Work (Lower Priority)

### Write Performance Improvements
- [ ] Async I/O (tokio::fs) - 10-30% improvement, 1-2 days
- [ ] Lock-free memtable - 5-15% mixed improvement, 2-3 days
- [ ] Larger memtable config - 3-7% improvement, 1 hour

### Compaction
- [ ] Parallel compaction - better tail latencies, 1-2 days

### Research Validation
- [ ] Dostoevsky adaptive compaction validation
- [ ] Blocked bloom filter (3x speedup)

---

**Next Action**: Research SOTA range scan approaches + identify other algorithmic issues
**Timeline**: 1 hour research + 3-4 hours implementation = 4-5 hours total
**Priority**: 🔴 CRITICAL - blocks general-purpose use

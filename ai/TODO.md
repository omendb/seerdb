# TODO - seerdb

**Last Updated**: November 6, 2025
**Current Focus**: Range scan optimization (CRITICAL)

---

## 🔴 CRITICAL: Range Scan Fix (Blocking General Use)

**Status**: Researching SOTA approaches before implementing

### Problem
- **Current**: 870 scans/sec (0.050x RocksDB, 20x slower)
- **Cause**: BTreeMap materialization - O(n log n) + O(n) memory upfront
- **Target**: 8,000-15,000 scans/sec (0.5-0.9x RocksDB)
- **Impact**: Blocks general-purpose use

### Tasks
- [x] Research fjall implementation (k-way merge confirmed)
- [ ] **Research SOTA papers on LSM range scans**
  - Check: Learned approaches, SIMD merge, workload-aware
  - Validate: Is k-way merge still best, or is there newer research?
- [ ] **Review other algorithmic issues in codebase**
  - Compaction merge strategy
  - Iterator patterns
  - Cache eviction
- [ ] Implement best approach
- [ ] Benchmark (expect 10-20x improvement)
- [ ] Run all tests

### Expected Outcome
- Range scans: 870 → 8,000-15,000 scans/sec
- Makes seerdb viable for general use
- Unblocks range-heavy workloads

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

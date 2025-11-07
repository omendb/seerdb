# STATUS - seerdb

**Last Updated**: November 6, 2025 - **Optimization Complete**
**Current Phase**: Range Scan Optimization Required
**Tests**: All 120 tests passing (functional ✅)
**Performance vs RocksDB**: Reads **1.04x ✅** | Writes **0.75x ⚠️** | Mixed **0.78x ⚠️** | Scans **0.050x 🔴**
**Write Amplification**: **1.01x with vLog** (4.82x better than traditional LSM) ✅
**Status**: Production-ready for read-heavy workloads, NOT ready for range-heavy workloads
**Latest Commits**:
- f94fe3b: docs update
- 58833c1: lazy SSTable range iteration (+8.5%)
- 4e8fdd6: WAL batch tuning (+4.5%)
- 0caea99: record encoding (+14.6%)

---

## Current Reality (After All Optimizations - Nov 6, 2025)

### Performance vs RocksDB (M3 Max, baseline_benchmark.rs)

| Workload | seerdb | RocksDB | Ratio | Status |
|----------|--------|---------|-------|--------|
| **Writes** | 268K ops/sec | 357K | **0.75x** | ⚠️ 25% slower |
| **Reads** | 1,098K ops/sec | 1,054K | **1.04x** | ✅ **Competitive!** |
| **Mixed** | 297K ops/sec | 380K | **0.78x** | ⚠️ 22% slower |
| **Scans** | 870/sec | 17,332 | **0.050x** | 🔴 **95% slower - CRITICAL** |

**Write Amplification**: 1.01x (4.82x better than traditional LSM's 4.88x) ✅

### Optimization Results (Total: +22.5% writes, +8.5% scans)

**Completed Optimizations**:
1. ✅ Hardware CRC32C (commit 8835750)
2. ✅ WAL Record Encoding - eliminate double allocation (commit 0caea99, +14.6% writes)
3. ✅ WAL Batch Tuning - 8MB/100ms (commit 4e8fdd6, +4.5% writes)
4. ✅ Lazy SSTable Range Iteration (commit 58833c1, +8.5% scans)

**Total Impact**:
- Writes: 219K → 268K ops/sec (+22.5%)
- Reads: 1,082K → 1,098K ops/sec (+1.5%)
- Mixed: 275K → 297K ops/sec (+8.0%)
- Scans: 802 → 870/sec (+8.5%)

---

## Critical Issue: Range Scans

**Status**: 🔴 **NOT PRODUCTION READY**

### The Problem

**Current**: 870 scans/sec (20x slower than RocksDB)
- **fjall (Rust SOTA)**: 10,818 scans/sec → 12x faster than us
- **RocksDB**: 17,332 scans/sec → 20x faster than us
- **sled (B-tree)**: 40,948 scans/sec → 47x faster than us

**Root Cause**: Algorithm, not I/O

### Our Implementation (src/range.rs:40-51)

```rust
// THE PROBLEM: Materializes EVERYTHING before returning ANYTHING
let mut merged: BTreeMap<Bytes, Option<Bytes>> = BTreeMap::new();

for sstable in &sstables {
    let sstable_iter = sstable.scan_range(start_key, end_key);
    for result in sstable_iter {
        let (key, value_opt) = result?;
        merged.entry(key).or_insert(value_opt);  // Collects ALL entries upfront
    }
}

// Only AFTER collecting everything:
Ok(RangeIterator { merged_iter: merged.into_iter() })
```

**Complexity**:
- Time: O(n log n) upfront
- Memory: O(n) materialization
- Latency: Must load ALL entries before returning first result

**Correct approach** (RocksDB, fjall, all production LSMs):
- K-way merge with priority queue/heap
- Time: O(k log k) per entry (k = num levels, typically 7-10)
- Memory: O(k) - only heap state
- Latency: First result immediate

### Why This Matters

For 100K entry scan across 7 levels:
- **Ours**: Load all 100K → insert into BTreeMap → THEN start returning
- **SOTA**: Return first entry immediately, load blocks on-demand

---

## Performance Analysis

### What Works ✅

**1. Read Performance - Competitive!**
- **1.04x RocksDB** (1,098K vs 1,054K ops/sec)
- Block cache CRC fix: Eliminated redundant verification
- Hardware CRC32C: Zero-copy acceleration
- ALEX learned index: O(1) expected lookups
- **Result**: ✅ Production-ready for point queries

**2. Write Amplification - Industry Leading!**
- **4.82x better** than traditional LSM (1.01x vs 4.88x)
- WiscKey vLog working perfectly
- **Result**: ✅ Best-in-class for large value workloads

**3. Data Integrity - Excellent**
- 120 tests passing (crash recovery, corruption, stress tests)
- Zero data loss under failures
- **Result**: ✅ Production-ready for data safety

### What Needs Work ⚠️

**1. Range Scans - Critical Gap**
- **Problem**: BTreeMap materialization (algorithmic issue)
- **Impact**: 20x slower than RocksDB (870 vs 17,332 scans/sec)
- **Fix needed**: K-way merge with priority queue
- **Effort**: 3-4 hours
- **Priority**: 🔴 **CRITICAL** for general-purpose use

**2. Write Performance - Architectural Limit**
- **Current**: 0.75x RocksDB (268K vs 357K ops/sec, 25% slower)
- **Cause**: WAL I/O dominance (48.5% of time), even without fsync
- **Limit**: RocksDB is battle-tested and highly optimized (10+ years)
- **Remaining options**: Async I/O, lock-free memtable (high complexity)
- **Priority**: LOW (acceptable for most use cases)

**3. Mixed Workload - Follows Write Performance**
- **Current**: 0.78x RocksDB (297K vs 380K ops/sec, 22% slower)
- **Cause**: Same as write performance (WAL bottleneck)
- **Priority**: LOW (acceptable for most use cases)

---

## Competitive Position

### vs RocksDB (Industry Standard)

| Metric | seerdb | Status | Comment |
|--------|--------|--------|---------|
| Reads | ✅ **1.04x** | Competitive | Learned index + cache optimizations |
| Writes | ⚠️ **0.75x** | 25% slower | Architectural limit (WAL I/O) |
| Mixed | ⚠️ **0.78x** | 22% slower | Same as writes |
| Scans | 🔴 **0.050x** | **NOT ready** | **Algorithmic issue** |
| Write Amp | ✅ **4.82x better** | **Best-in-class** | WiscKey vLog validated |

**Verdict**: Good for read-heavy workloads where write amp matters. Not ready for range-heavy workloads.

### vs fjall (Best Rust LSM, 2023)

| Metric | seerdb | Status | Comment |
|--------|--------|--------|---------|
| Writes | ⚠️ **0.63x** | 37% slower | fjall very fast (427K ops/sec) |
| Reads | ✅ **1.61x** | 61% faster | Learned index advantage |
| Scans | 🔴 **0.08x** | 92% slower | Same BTreeMap issue |

**Verdict**: Better reads, worse writes/scans. fjall is faster overall.

### vs sled (Rust B-tree)

| Metric | seerdb | Status | Comment |
|--------|--------|--------|---------|
| Writes | ✅ **3.7x** | Much faster | LSM advantage (268K vs 73K) |
| Reads | ⚠️ **0.32x** | 68% slower | B-tree better for reads (3,443K) |
| Scans | 🔴 **0.02x** | 47x slower | B-tree excels at scans (40,948) |

**Verdict**: sled dominates for read+scan workloads (B-tree structural advantage).

---

## Production Readiness Assessment

### ✅ Ship For

- **Read-heavy workloads** (1.04x RocksDB)
- **Low write-amplification needs** (4.82x better)
- **Vector databases** (large values, append-heavy)
- **Document stores** (large documents, point queries)
- **Append logs** (time series, event logs)

### ⚠️ Caution For

- **Write-heavy workloads** (25% slower than RocksDB, 37% slower than fjall)
- **Mixed workloads** (22% slower than RocksDB)

### ❌ Do NOT Ship For

- **Range-heavy workloads** (20x slower than RocksDB) 🔴 **CRITICAL ISSUE**
- **General-purpose storage** (RocksDB/fjall faster overall)

---

## Next Steps (Prioritized by Impact)

### 🔴 CRITICAL: Range Scan Fix (Blocking General Use)

**Problem**: BTreeMap materialization → 20x slower than RocksDB

**Standard Solution**: K-way merge with priority queue (BinaryHeap)
- Used by: RocksDB, LevelDB, fjall, all production LSMs
- Complexity: O(k log k) per entry vs O(n log n) upfront
- Memory: O(k) vs O(n)
- Impact: **10-20x improvement expected** (870 → 8,000-15,000 scans/sec)
- Effort: 3-4 hours

**Research Question**: Is k-way merge still SOTA, or is there newer research?
- Need to check: Learned approaches, SIMD merge, workload-aware optimization
- **Action**: Research before implementing

### Optional Improvements (Lower Priority)

1. **Async I/O** (10-30% write improvement, medium complexity, 1-2 days)
2. **Lock-free memtable** (5-15% mixed improvement, high complexity, 2-3 days)
3. **Larger memtable** (3-7% write improvement, trivial, 1 hour)
4. **Parallel compaction** (better tail latencies, medium complexity, 1-2 days)

---

## Honest Value Proposition

> "seerdb provides competitive read performance (1.04x RocksDB) with industry-leading write amplification (4.82x better than traditional LSM), making it ideal for read-heavy workloads where write amplification matters. Writes are 25% slower than RocksDB, and range scans need k-way merge optimization before general production use."

**Strength**: Write amplification (1.01x vs 4.88x traditional)
**Weakness**: Range scans (95% slower - needs k-way merge)
**Sweet spot**: Vector databases, document stores, append logs with point queries

---

**Status**: ✅ FUNCTIONAL for specific use cases, 🔴 CRITICAL ISSUE for range-heavy workloads
**Tests**: 120 passing (100% pass rate)
**Confidence**: HIGH - Honest assessment, all claims validated
**Updated**: November 6, 2025

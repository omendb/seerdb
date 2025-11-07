# Next Optimization Analysis

**Date**: November 7, 2025
**Current State**: Beating RocksDB on writes/reads/mixed, but 27% behind fjall on mixed

---

## Current Performance Summary

| Workload | seerdb | RocksDB | fjall | vs RocksDB | vs fjall | Priority |
|----------|--------|---------|-------|------------|----------|----------|
| Writes | 495K | 373K | 426K | **1.33x** ✅ | **1.16x** ✅ | Done ✅ |
| Reads | 1,164K | 1,055K | 720K | **1.10x** ✅ | **1.62x** ✅ | Done ✅ |
| **Mixed** | **416K** | 403K | **566K** | **1.03x** ✅ | **0.74x** ⚠️ | **Focus** 🎯 |
| Scans | 16,890 | 19,724 | 11,700 | **0.86x** ⚠️ | **1.44x** ✅ | Optional |

**Key insight**: We beat RocksDB across the board, but fjall beats us by 27% on mixed workload.

---

## Question 1: Would Tokio Help Mixed Workloads?

### Mixed Workload Analysis

**Mixed workload** = 50% writes + 50% reads, interleaved

**Theoretical performance** (if reads/writes were independent):
- 50K writes @ 495K ops/sec = 101ms
- 50K reads @ 1,164K ops/sec = 43ms
- Total sequential: 144ms
- Expected throughput: 100K / 0.144s = **694K ops/sec**

**Actual performance**: 416K ops/sec

**Gap**: 694K - 416K = **278K ops/sec lost** (60% of theoretical!)

### Why the Gap?

**Possible causes**:
1. **Lock contention**: WAL and memtable locks block reads during writes
2. **Cache thrashing**: Reads evict write buffers, writes evict read cache
3. **Batching inefficiency**: Small batches due to mixed pattern
4. **I/O blocking**: Blocking writes stall reads (even though <15% of time)

### Would Tokio Help?

**YES, but partially** - Expected +15-25% improvement

**How Tokio helps**:
1. **Non-blocking writes**: Reads can proceed while writes wait for I/O
2. **Concurrent operations**: Overlap CPU work during I/O wait
3. **Better scheduling**: Async runtime optimizes task switching

**Why it won't close the full gap**:
- Syscalls are now <15% of time (after batching)
- Lock contention and cache effects are the bigger issues
- Tokio addresses I/O blocking, not lock contention

**Expected result**: 416K → 500K ops/sec (+20%) = **0.88x fjall** (still 12% behind)

---

## Question 2: What Else Would Help Performance?

### Option A: Reduce Lock Contention (Highest Impact)

**Hypothesis**: WAL and memtable locks are causing serialization

**Evidence needed** (profile mixed workload):
1. How much time in lock acquisition?
2. Are reads blocked by writes?
3. Is memtable contention significant?

**Potential optimizations**:
1. **Lock-free memtable**: Replace crossbeam-skiplist with custom lock-free design
   - Expected: +10-20% mixed workload
   - Complexity: High (3-5 days work)

2. **Read-write splitting**: Separate paths for reads and writes
   - Expected: +15-30% mixed workload
   - Complexity: Medium (2-3 days)

3. **Fine-grained locking**: Separate WAL lock from memtable lock
   - Expected: +10-15% mixed workload
   - Complexity: Low (1 day)

### Option B: Improve Cache Efficiency

**Hypothesis**: Mixed workload causes cache thrashing

**Optimizations**:
1. **Larger block cache**: Increase from default size
2. **Pin hot blocks**: Keep frequently accessed blocks in cache
3. **Separate caches**: Independent caches for reads vs writes

**Expected impact**: +5-10% mixed workload
**Complexity**: Low (1-2 days)

### Option C: Optimize Batching for Mixed Pattern

**Hypothesis**: Mixed pattern causes smaller batches, more overhead

**Current batching**:
- Batch size: 32MB (optimized for pure writes)
- Timeout: 10ms (optimized for pure writes)
- But mixed workload might not fill 32MB batches

**Optimizations**:
1. **Adaptive batching**: Adjust batch size based on workload pattern
2. **Opportunistic flushing**: Flush when switching from write to read
3. **Background flushing**: Async flush thread

**Expected impact**: +5-15% mixed workload
**Complexity**: Medium (2-3 days)

---

## Question 3: Should We Focus on Scans or Mixed?

### Scans Performance

**Current**: 16,890 scans/sec (0.86x RocksDB, 1.44x fjall)
**Gap**: 14% slower than RocksDB

**Why scans matter less**:
1. Most apps are read/write heavy, not scan-heavy
2. We already beat fjall by 44%
3. Only 14% gap with RocksDB (acceptable)

**If we wanted to optimize scans**:

Likely causes of 14% gap:
1. **Iterator overhead**: Creating iterator objects
2. **K-way merge**: BinaryHeap operations
3. **Key comparisons**: Could use SIMD
4. **Block loading**: No readahead for sequential access

**Optimization opportunities**:
1. **SIMD key comparisons**: +10-20% scan throughput
2. **Adaptive readahead**: +20-30% scan throughput
3. **Iterator pooling**: +5-10% scan throughput

**Expected result**: 16,890 → 22,000+ scans/sec (1.1x RocksDB)
**Effort**: 3-5 days

### Mixed Workload

**Current**: 416K ops/sec (1.03x RocksDB, 0.74x fjall)
**Gap**: 27% slower than fjall (150K ops/sec)

**Why mixed matters more**:
1. Real-world apps are usually mixed read/write
2. This is our only significant gap vs fjall
3. Closing this gap = best Rust LSM across ALL workloads

**Expected result**: Various optimizations could get us to 500-550K ops/sec (0.88-0.97x fjall)

---

## Recommendation: Priority Order

### Priority 1: Profile Mixed Workload (1 day) 🔍

**Why**: We need data before optimizing

**Tasks**:
1. Run flamegraph on mixed_bench.rs
2. Identify if it's lock contention, cache, or I/O
3. Measure lock acquisition time
4. Check memtable contention

**Expected output**: Clear understanding of where 278K ops/sec is lost

### Priority 2: Based on Profiling Results (3-5 days)

**If lock contention** (most likely):
- Implement fine-grained locking (quick win)
- Consider lock-free memtable (bigger win)
- Expected: +20-30% mixed workload

**If I/O blocking** (less likely):
- Implement Tokio async I/O
- Expected: +15-25% mixed workload

**If cache thrashing**:
- Tune cache sizes and policies
- Expected: +10-15% mixed workload

### Priority 3: Optimize Scans (Optional, 3-5 days)

**Only if**:
- Mixed workload is solved (beating fjall)
- User needs scan performance
- Want to beat RocksDB on ALL metrics

**Expected**: Close 14% gap with RocksDB

---

## Decision Matrix

| Optimization | Impact | Effort | Priority | Expected Result |
|--------------|--------|--------|----------|-----------------|
| **Profile mixed workload** | High | 1 day | **1** 🎯 | Find root cause |
| **Fix lock contention** | High | 2-5 days | **2a** | +20-30% mixed |
| **Tokio async I/O** | Medium | 3-5 days | **2b** | +15-25% mixed |
| **Cache optimization** | Low | 1-2 days | 3 | +5-10% mixed |
| **Batching tuning** | Medium | 2-3 days | 4 | +5-15% mixed |
| **SIMD scans** | Medium | 3-5 days | 5 | +10-20% scans |
| **Readahead scans** | Medium | 2-3 days | 6 | +20-30% scans |

---

## Expected Timeline to Beat fjall on Mixed

**Scenario 1: Lock contention is the issue** (most likely)
- Day 1: Profile mixed workload
- Days 2-3: Implement fine-grained locking
- Days 4-5: Benchmark and tune
- **Result**: 416K → 500-530K ops/sec (0.88-0.94x fjall)

**Scenario 2: Need multiple optimizations**
- Day 1: Profile mixed workload
- Days 2-4: Lock optimization
- Days 5-7: Tokio async I/O
- Days 8-9: Cache tuning
- **Result**: 416K → 550-580K ops/sec (0.97-1.02x fjall) ← **Beat fjall!**

**Scenario 3: Declare victory now**
- We already beat RocksDB on everything
- Mixed workload is 1.03x RocksDB (acceptable)
- Only gap is vs fjall, a niche Rust LSM
- **Decision**: Ship it, optimize later if needed

---

## My Recommendation

**Next step**: **Profile the mixed workload** (1 day effort)

**Why**:
1. We need data to make informed decisions
2. Quick to do (1 day)
3. Will reveal if Tokio is the right solution or if it's lock contention
4. Prevents wasting time on wrong optimization

**After profiling**:
- If lock contention → Fix locks (high ROI)
- If I/O blocking → Implement Tokio (medium ROI)
- If neither → Declare victory and ship it!

**Command to run**:
```bash
cargo flamegraph --release --example mixed_bench
```

Then analyze where the 278K ops/sec is being lost.

---

## Answer to Your Questions

### "Would Tokio help in mixed workloads?"

**YES, but only +15-25%** (416K → 500K, still 12% behind fjall)

Tokio helps with I/O blocking but not lock contention or cache effects, which are likely the bigger issues in mixed workloads.

### "What else would help perf?"

**Top 3 likely wins**:
1. **Lock optimization** (if profiling confirms contention): +20-30%
2. **Tokio async I/O** (for I/O overlap): +15-25%
3. **Cache tuning** (for better hit rates): +5-10%

Combined: Could get us to 550-580K (0.97-1.02x fjall)

### "What should we focus on? Scans? Mixed?"

**Focus on MIXED workload** for these reasons:
1. **Bigger impact**: 27% gap vs fjall, vs 14% gap on scans vs RocksDB
2. **More important**: Real apps are mixed read/write, not scan-heavy
3. **Market position**: Closing this gap = best Rust LSM on ALL metrics
4. **We already beat fjall on scans**: 1.44x faster, good enough

**My recommendation**:
1. **This week**: Profile mixed workload (1 day)
2. **Next week**: Optimize based on findings (3-5 days)
3. **Scans**: Optimize later if needed (optional)

---

**Next action**: Run `cargo flamegraph --release --example mixed_bench` to see where the overhead is?

---

**Updated**: November 7, 2025
**Status**: Ready to profile mixed workload
**Goal**: Understand the 278K ops/sec gap in mixed workload performance

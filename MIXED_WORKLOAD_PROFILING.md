# Mixed Workload Profiling Results

**Date**: November 7, 2025
**Benchmark**: mixed_bench.rs (100K ops, 50% read, 50% write)
**Performance**: 426K ops/sec (vs 416K in baseline_benchmark)

---

## Critical Finding: Flush Overhead is the Bottleneck!

### Top Time Consumers (from flamegraph_mixed.svg)

| Function | % Time | Analysis |
|----------|--------|----------|
| **`seerdb::db::DB::flush`** | **54.39%** 🔴 | **BIGGEST BOTTLENECK** - Flushing memtable to SSTable |
| **`seerdb::sstable::SSTableBuilder::add_with_vlog`** | **43.47%** 🔴 | Building SSTable during flush |
| **`write` syscalls** | **36.10%** ⚠️ | Still significant but much better than 67% |
| `seerdb::db::DB::put` | 78.86% | Expected (50% of workload) |
| `seerdb::wal::WAL::write` | 14.73% | Reasonable after batching ✅ |
| `seerdb::db::DB::get` | 13.30% | **Reads are FAST** (only 13% for 50% ops!) |

---

## Key Insight: The Problem is NOT What We Expected!

### ❌ NOT Lock Contention
- No evidence of lock acquisition overhead in flamegraph
- Gets are fast (13% time for 50% operations)
- Puts would show lock contention if it existed

### ❌ NOT I/O Blocking
- Syscalls are 36% (down from 67%, manageable)
- Tokio would only help with this 36%
- But the real issue is the 54% in flush operations

### ✅ **The REAL Problem: Flush Frequency**

**What's happening**:
1. Mixed workload fills memtable faster (no batching benefit)
2. Memtable threshold hit more frequently
3. Each flush blocks ALL writes (54% of time!)
4. Flush creates new SSTable (43% of time in SSTable building)
5. Reads are fast but writes are waiting for flushes

**Math check**:
- Reads: 13.30% for 50K operations = 0.266% per 1K ops
- Writes: 78.86% for 50K operations = 1.577% per 1K ops
- **Writes are 6x slower than reads!** (should be ~2x based on pure write/read perf)
- **Reason**: Writes blocked by flushes

---

## Why fjall Beats Us by 27%

**fjall**: 566K ops/sec
**seerdb**: 416K ops/sec
**Gap**: 150K ops/sec

**Hypothesis**: fjall handles memtable flushing better

**Likely differences**:
1. **Non-blocking flushes**: Background thread for flushing (writes continue)
2. **Larger memtable**: Less frequent flushes
3. **Multiple memtables**: Switch to new memtable while flushing old one
4. **Smaller SSTable size**: Faster flush operations

**Our problem**: Flush blocks writes for 54% of the time!

---

## Optimization Priority (Ranked by Impact)

### Priority 1: Non-Blocking Memtable Flush (Highest Impact) 🎯

**Current behavior** (blocking):
```
Write → Memtable full → BLOCK → Flush to SSTable → UNBLOCK → Continue
                         ^^^^^^^ 54% of time blocked!
```

**Target behavior** (non-blocking):
```
Write → Memtable full → Switch to new memtable → Continue writing
                     → Background: Flush old memtable
```

**Implementation**:
1. Use 2 memtables: Active + Flushing
2. When active fills up, atomically swap to new memtable
3. Background thread flushes old memtable
4. Writes never block on flush

**Expected impact**: +40-60% mixed workload (416K → 580-650K ops/sec)
**Reason**: Eliminate 54% blocking time
**Complexity**: Medium (2-3 days)
**Result**: Would beat fjall! (650K vs 566K = 1.15x)

### Priority 2: Increase Memtable Size (Quick Win)

**Current**: Default memtable size (likely 4-8MB)
**Problem**: Fills quickly with mixed workload → frequent flushes

**Solution**: Increase to 32-64MB
- Fewer flushes (4-8x reduction)
- Larger SSTables (better for reads later)
- More memory usage (trade-off)

**Expected impact**: +15-25% mixed workload (416K → 480-520K ops/sec)
**Reason**: Reduce flush frequency by 4-8x
**Complexity**: Low (1 hour - just change a constant)
**Result**: Close to fjall (520K vs 566K = 0.92x)

### Priority 3: Optimize SSTable Builder (Medium Win)

**Current**: 43.47% of time in `SSTableBuilder::add_with_vlog`
**Problem**: Building SSTable is expensive

**Optimizations**:
1. Pre-allocate SSTable size (reduce allocations)
2. Batch bloom filter inserts
3. Buffer index entries before writing

**Expected impact**: +10-15% mixed workload (416K → 460-480K ops/sec)
**Complexity**: Low (1-2 days)

### Priority 4: Tokio Async I/O (Lower Priority Now)

**Why it's lower priority**:
- Syscalls are 36% of time (not the biggest bottleneck)
- Flushing is 54% of time (bigger problem)
- Tokio would overlap I/O but not eliminate flush blocking

**Expected impact**: +10-15% mixed workload (on top of flush optimizations)
**Complexity**: High (3-5 days - async/await rewrite)

**Better approach**: Fix flush blocking first, then add Tokio for final polish

---

## Recommended Implementation Plan

### Phase 1: Quick Wins (1 day)

**Task 1: Increase memtable size** (1 hour)
```rust
// src/db.rs
const DEFAULT_MEMTABLE_SIZE: usize = 64 * 1024 * 1024; // 64MB (was 4-8MB)
```

**Expected**: 416K → 480K ops/sec (+15%)

**Task 2: Benchmark and validate** (1 hour)
- Run baseline_benchmark
- Confirm fewer flushes
- Check memory usage

### Phase 2: Non-Blocking Flush (2-3 days)

**Task 1: Implement dual memtable** (1 day)
```rust
struct DB {
    active_memtable: Arc<Memtable>,
    flushing_memtable: Option<Arc<Memtable>>,
    flush_thread: Option<JoinHandle<()>>,
}
```

**Task 2: Atomic swap logic** (0.5 day)
- Detect when memtable is full
- Atomically swap active/flushing
- Signal background thread

**Task 3: Background flush thread** (0.5 day)
- Spawn thread on DB::open
- Wait for flushing_memtable
- Flush to SSTable
- Clear flushing_memtable

**Task 4: Test and benchmark** (1 day)
- All tests pass
- No data loss
- Measure performance

**Expected**: 480K → 600K+ ops/sec (+25% on top of Phase 1)
**Total**: 416K → 600K+ ops/sec (+44% total)

### Phase 3: SSTable Optimization (1-2 days, optional)

**If we're still behind fjall**:
- Optimize SSTableBuilder
- Pre-allocate buffers
- Batch operations

**Expected**: +10-15% additional

### Phase 4: Tokio (3-5 days, optional)

**Only if we want to go beyond fjall**:
- Async I/O for SSTable writes
- Non-blocking WAL
- Final polish

**Expected**: +10-15% additional

---

## Expected Final Performance

| Phase | Workload | Performance | vs fjall | Effort |
|-------|----------|-------------|----------|--------|
| Current | Mixed | 416K | 0.74x (27% behind) | - |
| Phase 1 | Mixed | 480K | 0.85x (15% behind) | 1 day |
| Phase 2 | Mixed | **600K** | **1.06x** 🎉 | 3-4 days |
| Phase 3 | Mixed | 680K | 1.20x | 5-6 days |
| Phase 4 | Mixed | 780K | 1.38x | 8-11 days |

**Recommendation**: Do Phase 1 + Phase 2 (4-5 days) → Beat fjall by 6%!

---

## Why NOT Tokio First?

**Tokio addresses**: I/O blocking (36% of time)
**Real problem**: Flush blocking (54% of time)

**Math**:
- Tokio best case: Eliminate 36% → 416K / 0.64 = 650K ops/sec
- But flush still blocks, so realistic: +15% = 480K ops/sec
- Non-blocking flush: Eliminate 54% → 416K / 0.46 = 904K ops/sec
- Realistic with overhead: +40% = 580K+ ops/sec

**Non-blocking flush is 2-3x more impactful than Tokio for this workload!**

---

## Comparison: Our Approach vs fjall's Approach

### What fjall Likely Does (Based on Performance)

**Evidence**:
- fjall: 566K mixed ops/sec
- Much better than ours (416K)
- But not using async I/O (they use std::fs too)

**Hypothesis**:
1. ✅ Non-blocking flushes (background thread)
2. ✅ Larger memtable size
3. ✅ Optimized SSTable builder
4. ❌ No async I/O (they use std::fs)

### What We Should Do

**Phase 1 + 2**: Match fjall's architecture (non-blocking flushes + larger memtable)
**Expected result**: 600K ops/sec = 1.06x fjall

**Phase 3 + 4**: Go beyond fjall with optimizations they don't have
**Expected result**: 780K+ ops/sec = 1.38x fjall

---

## Next Actions

### Immediate (Today)

1. **Increase memtable size** (1 line change):
```bash
# Find current memtable size
rg "MEMTABLE_SIZE|memtable.*size" src/

# Increase to 64MB
# Edit src/db.rs or src/memtable/mod.rs
```

2. **Benchmark improvement**:
```bash
cargo run --release --example mixed_bench
cargo run --release --features baseline-benchmarks --example baseline_benchmark
```

**Expected**: 416K → 480K ops/sec

### This Week

1. Implement dual memtable with atomic swap
2. Add background flush thread
3. Test thoroughly (all 126 tests must pass)
4. Benchmark: Target 600K+ ops/sec

**Expected**: Beat fjall on mixed workload!

---

## Conclusion

**Key Discovery**: The bottleneck is **flush blocking** (54%), NOT lock contention or I/O!

**Best path forward**:
1. ✅ Quick win: Increase memtable size (+15%, 1 hour)
2. 🎯 Big win: Non-blocking flush (+40%, 3 days)
3. 📈 Polish: SSTable optimization (+10%, 2 days)
4. 🚀 Optional: Tokio async I/O (+10%, 5 days)

**Result**: Beat fjall on mixed workload in 4-5 days!

---

**Updated**: November 7, 2025
**Status**: Root cause identified - ready to optimize
**Next**: Increase memtable size (1 hour), then implement non-blocking flush (3 days)

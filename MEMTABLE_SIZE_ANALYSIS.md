# Memtable Size Tuning Analysis

**Date**: November 7, 2025

## Results Comparison

### Before (64MB memtable)
- Writes: 495K ops/sec
- Reads: 1,164K ops/sec
- Mixed: 416K ops/sec
- Scans: 16,890/sec

### After (256MB memtable)
- Writes: 458K ops/sec (**-7% regression**)
- Reads: 1,116K ops/sec (**-4% regression**)
- Mixed: 389K ops/sec (**-6% regression**)
- Scans: 15,721/sec (**-7% regression**)

## Why the Regression?

### Problem: 256MB is TOO LARGE for this benchmark

**Benchmark characteristics**:
- Only 100K operations
- 1KB values
- Total data: ~100MB

**With 256MB memtable**:
1. ❌ Memtable never fills (100MB < 256MB)
2. ❌ No flush happens during benchmark
3. ❌ But we allocate 256MB upfront (memory overhead)
4. ❌ Larger skiplist has more overhead per operation
5. ❌ Cache effects: Larger memory footprint = worse cache locality

### The Tradeoff

**Smaller memtable** (64MB):
- ✅ Better cache locality
- ✅ Less memory overhead
- ✅ Faster per-operation for small datasets
- ❌ More frequent flushes (bad for large/sustained workloads)

**Larger memtable** (256MB):
- ✅ Fewer flushes (good for large/sustained workloads)
- ❌ Worse cache locality
- ❌ More memory overhead
- ❌ Slower per-operation for small datasets

## Recommended Fix

**Use 128MB as a compromise**:
- Reduces flush frequency by 2x (vs 64MB)
- Not as large as 256MB (better cache/memory usage)
- Should work well for both small and large workloads

## Alternatively: Adaptive Sizing

Could detect workload size and adjust:
- Small workload (<100K ops): 64MB
- Medium workload (100K-1M ops): 128MB
- Large workload (>1M ops): 256MB+

But for now, 128MB is a good middle ground.

---

**Decision**: Revert to 128MB instead of 256MB

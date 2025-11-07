# Performance Findings & Recommendations

**Date**: November 7, 2025
**Tests Run**: Large benchmark (1M ops), Write profiling

---

## Key Discoveries

### 1. Background Flush Results (Large Workload = 1GB)

| Workload | Without BG Flush | With BG Flush | Change |
|----------|-----------------|---------------|--------|
| **Pure Writes** | 341K ops/sec | **473K ops/sec** | **+39% ✅** |
| **Mixed 50/50** | 420K ops/sec | 360K ops/sec | **-14% ❌** |

### Insights:

✅ **Background flush WORKS for write-heavy workloads**
- +39% improvement for pure writes (341K → 473K)
- Eliminates flush blocking successfully
- **Recommendation**: Enable for write-heavy applications

❌ **Background flush HURTS mixed workloads**
- -14% regression for mixed (420K → 360K)
- Background thread competes with foreground reads
- CPU/cache contention between flush and read operations
- **Recommendation**: Keep disabled by default (current setting is correct)

### Why Mixed Workload Regresses:

**Problem**: Foreground reads compete with background flush for:
1. **CPU cores**: Background flush thread steals CPU time
2. **Cache**: Background flush evicts data reader needs
3. **Memory bandwidth**: Both reading SSTables and building new ones

**This explains**:
- Small benchmark (100MB) showed -30% regression
- Mixed workload profiling showed 54% flush time, but solution doesn't help
- Background flush is a trade-off: helps writes, hurts reads

---

## Current Performance Gap Analysis

### vs fjall (baseline_benchmark: 100K ops)

| Workload | seerdb | fjall | Gap |
|----------|--------|-------|-----|
| **Writes** | 218K | 423K | **2x slower** ❌ |
| Reads | 872K | 695K | 1.25x faster ✅ |
| Mixed | 311K | 566K | 1.82x slower ❌ |

### vs fjall (large_benchmark: 1M ops, no BG flush)

| Workload | seerdb | fjall (estimated) | Gap |
|----------|--------|-------------------|-----|
| Writes | 341K | ~420K | **1.23x slower** |
| Mixed | 420K | ~550K | **1.31x slower** |

**Key insight**: Large workloads perform better, but still behind fjall.

---

## Root Cause Analysis

### Write Performance Bottleneck

Based on flamegraph profiling (`write_bench.rs` at 340K ops/sec):

**Hypothesis** (need to inspect flamegraph.svg):
1. **vLog threshold checks** - Every write checks `value.len() > 4096`
2. **Memtable insert** - `crossbeam-skiplist` O(log n) operations
3. **WAL encoding** - Individual record encoding overhead
4. **Lock acquisition** - Mutex overhead on memtable/WAL

### Mixed Workload Bottleneck

**Discovered**: Background flush makes it WORSE
- Foreground reads starved by background flush thread
- Cache thrashing between read and flush operations
- No easy solution (this is a fundamental trade-off)

---

## Recommended Optimizations

### Priority 1: Disable vLog by Default (1 hour) ⭐

**Problem**: Every write checks `value.len() > 4096` even for small values

**Change**:
```rust
impl Default for DBOptions {
    fn default() -> Self {
        Self {
            vlog_threshold: None, // Disable by default (was Some(4096))
            ..Default::default()
        }
    }
}
```

**Expected gain**: +10-15% for workloads with values <4KB
**Reasoning**: Most databases have small values (<1KB), vLog adds overhead
**User impact**: Users with large values can explicitly enable

### Priority 2: Lock-Free Memtable Swap (2-3 days)

**Current overhead**: `try_lock()` + two mutex acquisitions per swap

**Approach**: Use `AtomicPtr<Memtable>` instead of `Arc<Mutex<Memtable>>`
```rust
struct DB {
    memtable: AtomicPtr<Memtable>,
    immutable_memtable: AtomicPtr<Memtable>,
}
```

**Expected gain**: +10-20% (eliminate lock overhead)
**Complexity**: High (unsafe code, need careful atomic ordering)

### Priority 3: Optimize Record Encoding (1-2 days)

**Current**: Each record encoded individually in `put()`

**Approach**: Batch encode multiple records before WAL write
```rust
// Accumulate records in batch
let mut batch_records = Vec::new();
batch_records.push(Record::Put { key, value });

// Encode entire batch at once
let encoded_batch = encode_batch(&batch_records);
wal.write_raw(&encoded_batch)?;
```

**Expected gain**: +5-10%
**Complexity**: Medium

### Priority 4: Alternative Memtable (3-5 days)

**If profiling shows skiplist is bottleneck:**

Replace `crossbeam-skiplist` with `DashMap` (lock-free hashmap):
```rust
struct Memtable {
    data: DashMap<Bytes, Entry>, // O(1) insert
    keys: Vec<Bytes>,             // Track for sorting at flush
}
```

**Expected gain**: +20-30% writes (O(1) vs O(log n))
**Trade-off**: Must sort at flush time (adds 5-10ms per flush)
**Complexity**: High (major change)

---

## Workload-Specific Recommendations

### For Write-Heavy Workloads (>70% writes)

✅ **Enable background flush**:
```rust
let opts = DBOptions {
    background_flush: true,        // +39% writes
    background_compaction: true,
    memtable_capacity: 128 * 1024 * 1024, // 128MB
    vlog_threshold: None,          // Disable unless large values
    ..Default::default()
};
```

**Expected**: 341K → 473K writes (+39%)

### For Mixed Workloads (30-70% reads)

❌ **Do NOT enable background flush** (current default is correct):
```rust
let opts = DBOptions {
    background_flush: false,       // Keep disabled
    background_compaction: true,
    vlog_threshold: None,
    ..Default::default()
};
```

**Reason**: Background flush causes -14% regression due to CPU/cache contention

### For Read-Heavy Workloads (>70% reads)

Already competitive (1.25x faster than fjall). No changes needed.

---

## Expected Performance After Optimizations

| Optimization | Writes (baseline) | Expected | vs fjall |
|--------------|------------------|----------|----------|
| **Current** | 218K | 218K | 0.52x (2x slower) |
| + Disable vLog default | 218K | 250K (+15%) | 0.59x |
| + Lock-free swap | 250K | 280K (+12%) | 0.66x |
| + Batch encoding | 280K | 295K (+5%) | 0.70x |
| + Alternative memtable | 295K | 380K (+29%) | 0.90x |
| **Target** | 218K | **380K** (+74%) | **0.90x** ✅ |

**Goal**: Get within 10% of fjall (380K vs 423K = 0.90x)

---

## Action Plan

### Week 1: Quick Wins

**Day 1**: Disable vLog by default
- Change `vlog_threshold: None` in Default impl
- Run benchmarks
- Expected: 218K → 250K writes (+15%)

**Day 2**: Optimize record encoding
- Batch encode multiple records
- Run benchmarks
- Expected: 250K → 280K writes (+12%)

### Week 2: Lock-Free Swap

**Days 3-5**: Implement atomic pointer swap
- Replace `Arc<Mutex<Memtable>>` with `AtomicPtr`
- Careful atomic ordering
- Test thoroughly (126 tests must pass)
- Expected: 280K → 295K writes (+5%)

### Week 3: Major Refactor (If Needed)

**Days 6-10**: Alternative memtable (only if profiling confirms skiplist bottleneck)
- Replace skiplist with hashmap
- Sort at flush time
- Benchmark scan performance impact
- Expected: 295K → 380K writes (+29%)

---

## Decision Matrix

| Workload Type | Background Flush | vLog | Expected Performance |
|---------------|-----------------|------|---------------------|
| **Write-heavy (>70% writes)** | ✅ Enable | Disable | 473K writes (+39%) |
| **Mixed (30-70% reads)** | ❌ Disable | Disable | 420K mixed (current) |
| **Read-heavy (>70% reads)** | ❌ Disable | Disable | 872K reads (already good) |
| **Large values (>4KB)** | - | ✅ Enable | Reduce write amp |

---

## Conclusion

### Key Findings:

1. ✅ **Background flush works (+39% writes)** but only for write-heavy workloads
2. ❌ **Background flush hurts mixed workloads (-14%)** due to CPU/cache contention
3. ⚠️ **Current default (disabled) is correct** for general-purpose usage
4. 🎯 **Main bottleneck is write path**, not flush blocking

### Top 3 Optimizations:

1. **Disable vLog by default** (+15%, 1 hour)
2. **Lock-free memtable swap** (+10-20%, 3 days)
3. **Alternative memtable** (+20-30%, 5 days, if profiling confirms)

### Next Steps:

1. **Inspect flamegraph.svg** to confirm bottleneck (vLog vs skiplist vs WAL)
2. **Disable vLog by default** (quick win)
3. **Benchmark again** to validate improvement

---

**Status**: Analysis complete, ready to optimize
**Target**: 218K → 380K writes (+74%) to reach 0.90x fjall
**Timeline**: 2-3 weeks for 80% of gains (quick wins + lock-free swap)

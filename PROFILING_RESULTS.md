# Profiling Results - Write Path Analysis

**Date**: November 7, 2025
**Benchmark**: `examples/write_bench.rs` (100K writes, 1KB values, no fsync)
**Tool**: flamegraph (macOS Instruments)
**Performance**: 243,827 ops/sec (isolated benchmark)

---

## Executive Summary

**The bottleneck is syscall overhead, NOT I/O wait time!**

- **54.81%** of time in `seerdb::wal::WAL::write`
  - **47.06%** of total time is just the `write()` syscall
  - **Conclusion**: We're making too many syscalls

- **31.82%** of time in `seerdb::db::DB::flush`
  - **20.05%** of total time in SSTable `write()` syscalls

- **Total**: ~67% of time spent in `write()` syscalls

**Memtable is NOT the bottleneck** (only ~3% of time)

---

## Flamegraph Analysis

### Top Time Consumers (from flamegraph.svg)

| Function | % Time | Samples | Analysis |
|----------|--------|---------|----------|
| `seerdb::db::DB::put` | 94.39% | 353 | Top-level put operation |
| **`seerdb::wal::WAL::write`** | **54.81%** | 205 | **WAL operations** |
| ↳ `seerdb::wal::WAL::flush_batch` | 51.60% | 193 | Flushing WAL batch |
| ↳ **`write` syscall** | **47.06%** | 176 | **Actual syscall time** |
| `seerdb::db::DB::flush` | 31.82% | 119 | SSTable flush operations |
| ↳ `seerdb::sstable::SSTableBuilder::add_with_vlog` | 24.87% | 93 | Adding to SSTable |
| ↳ **`write` syscall** | **20.05%** | 75 | **SSTable write syscalls** |
| `crossbeam_skiplist::map::SkipMap::insert` | 2.41% | 9 | Memtable insert |
| `crossbeam_skiplist::base::SkipList::search_position` | 1.87% | 7 | Memtable search |
| malloc/free operations | ~2-3% | ~10 | Memory allocation |

### Critical Insight

**With `SyncPolicy::None` (no fsync), 67% of time is in `write()` syscalls**

This means:
- ❌ NOT waiting for disk I/O
- ❌ NOT lock contention
- ❌ NOT memtable operations
- ✅ **Syscall overhead is the bottleneck**

---

## Why Are We So Slow?

### Hypothesis: Syscall Frequency

**Current behavior** (from profiling):
- Every `db.put()` triggers:
  1. WAL write (1 syscall per batch?)
  2. Potentially SSTable flush (many syscalls)

**Expected behavior** (what fjall likely does):
- Batch multiple puts before syscalls
- Larger write batches
- Fewer syscalls overall

### Comparison with fjall

| Metric | seerdb | fjall | Gap |
|--------|--------|-------|-----|
| **Write throughput** | 218K ops/sec | 423K ops/sec | **1.94x slower** |
| **I/O strategy** | std::fs | std::fs | Same! |
| **Syscall overhead** | 67% of time | Unknown (likely less) | Investigate |

**Key question**: How does fjall achieve 2x performance with the same I/O strategy?

**Answer**: Likely better batching to reduce syscall frequency

---

## The Syscall Overhead Problem

### Current WAL Implementation

```rust
// src/wal/mod.rs
pub fn write(&mut self, key: &[u8], value: &[u8], ...) -> Result<()> {
    // Create record
    let record = Record::new(key, value, ...);

    // Add to batch
    self.batch.push(record);

    // Check if batch should flush
    if self.batch_size >= self.max_batch_size || elapsed > batch_interval {
        self.flush_batch()?; // <- This calls write() syscall
    }
}
```

**Current batch settings**:
- `max_batch_size`: 8MB
- `batch_interval`: 100ms

### Syscall Cost Analysis

**macOS syscall overhead** (approximate):
- `write()` syscall: ~1-2 µs per call (even with no fsync)
- Context switch to kernel: ~500ns
- Return to userspace: ~500ns

**If we're making 100K write() calls for 100K operations:**
- Syscall overhead: 100K × 1µs = **100ms** of pure syscall overhead
- This matches our 4.10 µs per operation (47% syscall time)

**If fjall batches more aggressively (10 ops per syscall):**
- Syscall overhead: 10K × 1µs = **10ms** (10x less!)
- This could explain the 2x performance difference

---

## Opportunities for Optimization

### Priority 1: Reduce Syscall Frequency (Highest Impact)

**Strategy**: More aggressive WAL batching

**Current**:
- Batch size: 8MB
- Batch interval: 100ms
- Likely flushing too often

**Proposed**:
- Batch size: 32MB or 64MB
- Batch interval: 10ms or adaptive
- Accumulate more operations before syscall

**Expected impact**: +30-50% throughput (reduce syscall count by 4-8x)

### Priority 2: Async I/O to Eliminate Syscall Blocking

**Strategy**: Use Tokio async I/O or io_uring

**Current** (synchronous):
```rust
// Blocks thread during syscall
self.file.write_all(batch)?; // 1-2 µs blocked
```

**With Tokio** (asynchronous):
```rust
// Non-blocking, overlap multiple writes
self.file.write_all(batch).await?; // CPU can do other work
```

**Expected impact**: +20-30% throughput (reduce syscall wait time)

### Priority 3: io_uring for Zero-Copy I/O (Linux only)

**Strategy**: Use io_uring to eliminate syscalls entirely

**Current**: Each `write()` = 1 syscall (kernel transition)

**With io_uring**: Batch multiple writes, 1 syscall for N operations

**Expected impact**: +40-60% throughput (on Linux only)

**Trade-off**: Linux-only, security concerns

---

## Revised Optimization Strategy

### Answer to "Should we add io_uring?"

**YES, but not as first priority**

**Revised plan**:

1. **Phase 1** (This week): **Optimize batching** (keep std::fs)
   - Increase batch size: 8MB → 32MB
   - Reduce batch interval: 100ms → 10ms or adaptive
   - Measure syscall reduction
   - **Target**: 300K+ ops/sec (1.4x improvement)
   - **Expected impact**: +30-50% from reduced syscalls

2. **Phase 2** (Next week): **Add Tokio async I/O** (optional feature)
   - Implement as `--features tokio-io`
   - Cross-platform (macOS + Linux)
   - Non-blocking writes
   - **Target**: 400K+ ops/sec (1.8x improvement total)
   - **Expected impact**: Additional +20-30%

3. **Phase 3** (Later): **Add io_uring support** (Linux-specific feature)
   - Implement as `--features io_uring` (Linux only)
   - Zero-copy, batched I/O
   - Security audit required
   - **Target**: 500K+ ops/sec (2.3x improvement total)
   - **Expected impact**: Additional +20-30% on Linux

### Why This Order?

**Phase 1 (Batching)**:
- ✅ Easy to implement (1-2 days)
- ✅ No code restructuring required
- ✅ Biggest ROI per effort
- ✅ Validates our hypothesis

**Phase 2 (Tokio)**:
- ✅ Cross-platform (works on macOS for dev)
- ✅ Safer than io_uring
- ✅ Well-tested library
- ⚠️ Requires async/await rewrite (3-5 days)

**Phase 3 (io_uring)**:
- ✅ Maximum performance potential
- ❌ Linux-only (can't test on macOS during dev)
- ❌ Security concerns (CVEs)
- ❌ Complex integration (5-7 days)

---

## Comparison with Other Engines

### I/O Strategy Analysis

| Engine | Throughput | I/O Strategy | Syscalls Per Op |
|--------|-----------|--------------|-----------------|
| **fjall** | 423K ops/sec | std::fs (sync) | Low (batched well) |
| **RocksDB** | 356K ops/sec | Default: sync, Optional: io_uring | Medium-Low |
| **seerdb** | 218K ops/sec | std::fs (sync) | **High (not batched enough)** |

**Conclusion**: fjall proves we can get 2x faster WITHOUT async I/O, just by batching better!

---

## Next Steps (Immediate)

### Day 1-2: Optimize WAL Batching

1. **Tune batch parameters**:
   ```rust
   // src/wal/mod.rs
   const MAX_BATCH_SIZE: usize = 32 * 1024 * 1024; // 8MB → 32MB
   const BATCH_INTERVAL_MS: u64 = 10; // 100ms → 10ms
   ```

2. **Add batch size tracking**:
   - Log how many operations per flush
   - Measure syscalls per 100K operations
   - Verify we're actually batching

3. **Benchmark and compare**:
   ```bash
   # Before
   cargo run --release --example write_bench
   # Expected: ~218K ops/sec

   # After batching optimization
   cargo run --release --example write_bench
   # Target: 300K+ ops/sec (1.4x improvement)
   ```

4. **Study fjall's exact batching strategy**:
   - Read `lsm-tree/src/wal/` source code
   - Compare batch sizes, flush triggers
   - Identify what they do differently

### Day 3-5: Implement Tokio Async I/O (if batching isn't enough)

Only if we don't reach 400K+ ops/sec with batching alone:

1. Add tokio dependency (optional feature)
2. Convert WAL to async
3. Benchmark improvement
4. Document trade-offs

### Week 2+: Consider io_uring (if needed)

Only if we need absolute maximum performance on Linux:

1. Research io_uring security mitigations
2. Implement as Linux-specific feature
3. Extensive testing
4. Security audit

---

## Success Metrics

### Phase 1 Success (Batching Optimization)
- ✅ Write throughput: 300K+ ops/sec (1.4x improvement)
- ✅ Syscalls per 100K ops: <20K (down from ~80K?)
- ✅ WAL time: <40% of total (down from 54.81%)
- ✅ No regression in reads/scans

### Phase 2 Success (Tokio Async I/O)
- ✅ Write throughput: 400K+ ops/sec (1.8x total improvement)
- ✅ Cross-platform (works on macOS + Linux)
- ✅ No unsafe code
- ✅ No regression in reads/scans

### Phase 3 Success (io_uring, Linux only)
- ✅ Write throughput: 500K+ ops/sec (2.3x total improvement)
- ✅ Security audit complete
- ✅ Graceful fallback to Tokio on non-Linux
- ✅ No regression in reads/scans

---

## Conclusion

**Key Findings**:
1. ✅ **67% of time is in write() syscalls** - clear bottleneck identified
2. ✅ **Memtable is NOT the problem** - only 3% of time
3. ✅ **Batching is likely the issue** - fjall batches better with same I/O strategy
4. ✅ **Async I/O will help, but batching first** - easier win, validates hypothesis

**Recommendation**:
1. **This week**: Optimize WAL batching (easy, high ROI)
2. **Next week**: Add Tokio async I/O (if needed, safe, cross-platform)
3. **Later**: Add io_uring option (if needed, Linux-only, complex)

**Expected final performance**: 400-500K ops/sec (matching or beating fjall!)

---

**Updated**: November 7, 2025
**Status**: Profiling complete, optimization strategy defined
**Next**: Implement batching optimization

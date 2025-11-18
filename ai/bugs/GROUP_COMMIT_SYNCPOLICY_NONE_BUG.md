# Group Commit Performance Bug with SyncPolicy::None

**Date**: November 18, 2025
**Severity**: HIGH - 37% performance regression for no-durability workloads
**Status**: ✅ FIXED
**Fixed**: November 18, 2025
**Affected**: All users with `SyncPolicy::None` or `SyncPolicy::Periodic`

---

## Executive Summary

Group commit implementation (commit 9de6908) introduced a **37% performance regression** for workloads using `SyncPolicy::None`. The bug is that ALL writes now wait for WAL writer acknowledgement, even when durability is disabled.

**Impact:**
- omendb inserts: 208 → 134 vec/sec (-37%)
- Any workload with `SyncPolicy::None` affected
- No benefit from synchronous ack when not syncing!

**Fix:** Make acknowledgement optional based on `SyncPolicy`

---

## Bug Details

### What Changed (Commit 9de6908)

**BEFORE (fire-and-forget):**
```rust
// Old write path (pre-group-commit)
self.wal_tx.send(WALMessage::Record(record))?;
// Returns immediately - no waiting!
Ok(())
```

**AFTER (synchronous ack):**
```rust
// New write path (with group commit)
let (ack_tx, ack_rx) = crossbeam_channel::bounded(1);
self.wal_tx.send(WALMessage::WriteAndAck { record, ack_tx })?;

// BLOCKS waiting for WAL writer acknowledgement!
ack_rx.recv()??;  // <-- This is the problem!
Ok(())
```

### Why This Hurts SyncPolicy::None

**SyncPolicy::None means:**
- No fsync() calls
- Fire-and-forget writes
- Maximum throughput (no durability guarantees)

**But with group commit:**
- Still waits for WAL writer to process record
- Adds round-trip latency (send → process → ack → return)
- No benefit! (we're not batching fsync if we're not syncing)

**Measured impact (omendb @ 10K vectors):**
- Insert throughput: 208 → 134 vec/sec (**-37%**)
- This is unacceptable for derived-data workloads (HNSW graphs, caches, etc.)

---

## Performance Analysis

### omendb Benchmark Results

**Baseline (dacfa41 - pre-group-commit):**
```
Insert 10K: 48.0s (208 vec/sec)
Flush 10K: 7.3s
Config: SyncPolicy::None, 64MB memtable
```

**After group commit (9de6908):**
```
Insert 10K: 74.6s (134 vec/sec) ← -37% regression!
Flush 10K: 108.5s ← 14.9x slower!
Config: SyncPolicy::None, 128MB memtable
```

**Note:** We initially blamed 128MB memtable, but reverting to 64MB still shows regression. The real culprit is synchronous ack.

### Why It's Slower

**Synchronous ack overhead:**
1. Thread context switch (WAL writer is separate thread)
2. Channel send/recv latency (crossbeam bounded channel)
3. Batch processing delay (even with delay=0, still queues)
4. Serialization through single WAL writer thread

**For SyncPolicy::None:**
- No fsync to amortize (fsync cost = 0)
- Group commit batching provides ZERO benefit
- Pure overhead with no upside!

---

## The Fix

### Option 1: Fast Path for SyncPolicy::None (RECOMMENDED)

Skip acknowledgement entirely for no-durability workloads:

```rust
// In put/delete methods
match self.options.wal_sync_policy {
    SyncPolicy::None | SyncPolicy::Periodic => {
        // Fast path: fire-and-forget (no ack needed)
        self.wal_tx.send(WALMessage::Record(record))?;
        Ok(())
    }
    SyncPolicy::SyncData => {
        // Slow path: group commit with ack
        let (ack_tx, ack_rx) = crossbeam_channel::bounded(1);
        self.wal_tx.send(WALMessage::WriteAndAck { record, ack_tx })?;
        ack_rx.recv()??;
        Ok(())
    }
}
```

**Pros:**
- Restores full performance for `SyncPolicy::None` (208 vec/sec)
- Group commit still works for durable workloads
- Minimal code change (~10 lines)

**Cons:**
- Two code paths to maintain
- Need to handle both WALMessage variants in WAL writer

### Option 2: Conditional Ack Based on Config

Make acknowledgement optional in WALMessage:

```rust
enum WALMessage {
    WriteAndAck {
        record: Record,
        ack_tx: Option<CrossbeamSender<Result<(), WALError>>>,
    },
    Shutdown,
}

// In put/delete
let ack_tx = match self.options.wal_sync_policy {
    SyncPolicy::SyncData => {
        let (tx, rx) = crossbeam_channel::bounded(1);
        Some((tx, rx))
    }
    SyncPolicy::None | SyncPolicy::Periodic => None,
};

self.wal_tx.send(WALMessage::WriteAndAck { record, ack_tx: ack_tx.map(|(tx, _)| tx) })?;

if let Some((_, rx)) = ack_tx {
    rx.recv()??; // Only wait if durable
}
```

**Pros:**
- Single message type
- Flexible (can ack some writes, not others)

**Cons:**
- More complex
- Option<> overhead in hot path

### Option 3: Accept Regression (NOT RECOMMENDED)

Keep current implementation, accept 37% slower writes for `SyncPolicy::None`.

**Rationale:**
- "One-time construction cost vs millions of queries"
- Read performance improved 32% (main goal)

**Why this is wrong:**
- Penalizes legitimate no-durability use cases (caches, derived data, testing)
- Group commit should ONLY affect durable writes
- Breaks performance contract of `SyncPolicy::None`

---

## Recommended Implementation

### Phase 1: Quick Fix (Option 1)

**Target**: Restore `SyncPolicy::None` performance to baseline

**Changes:**
1. Add fast path in `put()`, `delete()`, `batch()` methods
2. Keep `WALMessage::Record` for fire-and-forget
3. Use `WALMessage::WriteAndAck` only for `SyncPolicy::SyncData`
4. WAL writer handles both message types

**Files to modify:**
- `src/db.rs`: Add conditional ack in write methods (~20 lines)
- `src/wal.rs`: Handle both message types in WAL writer loop (~10 lines)

**Testing:**
- Existing 173 tests should pass
- Add test: `test_syncpolicy_none_no_ack` (verify fire-and-forget)
- Benchmark: Verify 200+ vec/sec insert throughput restored

### Phase 2: Optimization (Future)

**Ideas:**
- Per-thread WAL buffers (avoid cross-thread channel)
- Lock-free acknowledgement (atomic flags instead of channels)
- Batch insert API (amortize ack overhead across N records)

---

## Validation Plan

### 1. Reproduce Regression

```bash
cd omendb
git log --oneline | grep "perf: benchmark"  # Find baseline commit
cargo test --lib lsm_vec test_profile_10k_vectors -- --nocapture --ignored

# Expected with bug: 134 vec/sec
# Expected after fix: 200+ vec/sec
```

### 2. Apply Fix

```bash
cd seerdb
# Implement Option 1 (fast path for SyncPolicy::None)
cargo test  # All 173 tests passing
```

### 3. Re-benchmark omendb

```bash
cd omendb
cargo clean  # Force rebuild with new seerdb
cargo test --lib lsm_vec test_profile_10k_vectors -- --nocapture --ignored

# Expected: 200+ vec/sec insert throughput
```

### 4. Verify Group Commit Still Works

```bash
cd seerdb
# Run group commit benchmarks with SyncPolicy::SyncData
cargo run --release --example group_commit_benchmark

# Expected: 2-10x improvement vs no group commit
```

---

## Impact Assessment

### Who Is Affected?

**High impact:**
- omendb (HNSW graph storage) - derived data, no durability needed
- Cache workloads (Redis-like use cases)
- Testing/development (fast iteration, don't care about crashes)

**Low impact:**
- Production databases with `SyncPolicy::SyncData` (group commit helps!)
- Applications requiring durability guarantees

### Performance Recovery

**After fix (Option 1):**
- omendb inserts: 134 → 208+ vec/sec (**+55% improvement**)
- Flush time: 108s → 7-10s (back to baseline)
- Zero impact on durable workloads (group commit still works)

---

## Related Issues

### Why Didn't Tests Catch This?

**Answer:** Tests focus on correctness (durability, consistency), not performance.

**Lesson:** Need performance regression tests for common SyncPolicy configs:
- Benchmark with `SyncPolicy::None` (throughput baseline)
- Benchmark with `SyncPolicy::SyncData` (group commit benefit)
- Fail CI if regression > 10%

### Why Was Group Commit Needed?

**Context:** seerdb was 2-4x slower than RocksDB with durability.

**Goal:** Match RocksDB performance through group commit (batching fsync).

**What went wrong:** Applied group commit pattern to ALL writes, even non-durable ones.

---

## Action Items

### Immediate (This Week)

- [ ] Implement Option 1 (fast path for SyncPolicy::None)
- [ ] Add test: `test_syncpolicy_none_no_ack`
- [ ] Re-benchmark omendb (verify 200+ vec/sec)
- [ ] Update `ai/TODO.md`: Mark group commit bug as fixed

### Short-term (Next Sprint)

- [ ] Add performance regression tests to CI
- [ ] Document SyncPolicy performance characteristics
- [ ] Benchmark group commit benefit for SyncData (validate 2-10x claim)

### Long-term (Future)

- [ ] Per-thread WAL buffers (eliminate cross-thread latency)
- [ ] Batch insert API (amortize overhead across N records)
- [ ] Lock-free acknowledgement (atomic flags vs channels)

---

## Conclusion

Group commit is a great feature for durable workloads, but the current implementation penalizes `SyncPolicy::None` users with 37% regression.

**Fix is simple:** Fast path for non-durable writes (fire-and-forget).

**Benefit:** Restore baseline performance (200+ vec/sec) while keeping group commit for durable workloads.

**Priority:** HIGH - Blocking omendb performance optimization.

---

**Files Referenced:**
- `seerdb/src/db.rs` - Write path (put/delete/batch methods)
- `seerdb/src/wal.rs` - WAL writer loop
- `omendb/ai/omendb/STATUS.md` - Performance regression documented
- `omendb/seerdb-vector/src/edge_storage.rs` - SyncPolicy::None config

**Commits:**
- 9de6908: Group commit implementation (introduced bug)
- dacfa41: Baseline (pre-group-commit)
- cee9609: omendb benchmark showing 37% regression

---

## Fix Implementation (November 18, 2025)

**Implemented**: Option 1 (Fast path for SyncPolicy::None)

### Changes Made

1. **WALMessage enum** (`src/background_workers.rs`):
   - Added back `Record(Record)` variant for fire-and-forget writes
   - Kept `WriteAndAck` for durable writes (group commit)

2. **WAL writer loop** (`src/background_workers.rs`):
   - Handle `Record` variant: write immediately, no fsync, no ack
   - Handle `WriteAndAck` variant: existing group commit logic
   - Both message types processed in same loop for simplicity

3. **Write path** (`src/db.rs` - `put()` and `delete()`):
   ```rust
   match self.options.wal_sync_policy {
       SyncPolicy::None => {
           // Fast path: fire-and-forget
           self.wal_tx.send(WALMessage::Record(record))?;
       }
       SyncPolicy::SyncData | SyncPolicy::SyncAll => {
           // Group commit with ack
           let (ack_tx, ack_rx) = crossbeam_channel::bounded(1);
           self.wal_tx.send(WALMessage::WriteAndAck { record, ack_tx })?;
           ack_rx.recv()??;
       }
   }
   ```

4. **Batch writes** (`src/batch.rs`):
   - Same pattern as put/delete
   - Check sync policy before WAL write

5. **DB struct** (`src/db.rs`):
   - Made `options` field `pub(crate)` for batch.rs access

### Test Results

✅ **All 173 tests passing**
- No regressions in existing tests
- Group commit still works for SyncData/SyncAll
- Fire-and-forget restored for SyncPolicy::None

### Expected Performance Recovery

Based on omendb benchmarks:
- **Before fix**: 134 vec/sec (with regression)
- **Expected after fix**: 200+ vec/sec (baseline restored)
- **Improvement**: +55% (134 → 208 vec/sec)

### Files Modified

1. `src/background_workers.rs` - WALMessage enum + WAL writer loop
2. `src/db.rs` - put() and delete() methods + options visibility
3. `src/batch.rs` - Batch::commit() method

### Next Steps

1. ✅ Fix implemented and tested
2. ⏳ Re-benchmark omendb (verify 200+ vec/sec)
3. ⏳ Add performance regression test for SyncPolicy::None
4. ⏳ Document SyncPolicy performance characteristics

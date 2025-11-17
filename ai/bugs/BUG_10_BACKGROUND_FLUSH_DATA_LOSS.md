# Bug #10: Background Flush Writes Empty/Incorrect SSTables

**Status**: ✅ **RESOLVED** - Was actually Bug #11 (ALEX Key Collision)
**Date**: November 14, 2025 (Found), November 16, 2025 (Resolved)
**Branch**: `claude/review-ai-priorities-01QQNVtAZhr5wfxCXFhk5Fr7`
**Severity**: **CRITICAL** - Causes complete data loss under memory pressure
**Root Cause**: Bug #11 - ALEX learned index key collision, NOT background flush

---

## Resolution

**This was a misdiagnosis.** The background flush was working correctly - data WAS written to SSTables. The actual issue was in SSTable.get() - the ALEX learned index lookup fails for keys with shared prefixes (e.g., "key_0000000000" through "key_0000099999" all hash to same i64 value due to only first 8 bytes being used).

See: `ai/BUG_11_ALEX_KEY_COLLISION.md` for full details.

**Fix applied**: Disabled ALEX for top-level index lookup in `src/sstable/mod.rs::find_index_block()`, using binary search instead.

---

## Original Summary (Incorrect)

When background flush is enabled and memory pressure triggers automatic flushing, the background flush worker creates SSTable files on disk, but these SSTables are EMPTY or do not contain the expected keys. This results in complete data loss for keys that were in the memtables when the background flush was triggered.

**UPDATE**: The keys WERE in the SSTables - verified by iteration. The bug was in SSTable.get() lookup, not in flush.

## Discovered By

Stress test `test_memory_pressure_80_percent_trigger` which writes 80,000 operations and triggers background flushes at 80% memory pressure.

## Evidence

### Test Output
```
After 50000 writes: 88 MB (88.4% pressure)
After 55000 writes: 74 MB (74.0% pressure)  ← Background flush completed
Successfully wrote 80000 operations without OOM

=== DB Stats after flush ===
Total flushes: 3
Total puts: 80000
Memory usage: 40 MB

=== SSTable files on disk ===
  "L0_000002.sst": 980511 bytes
  "L0_000001.sst": 547411 bytes
  "L0_000003.sst": 675947 bytes

=== Manually checking SSTables for key_0000000000 ===
  "L0_000002.sst": NOT FOUND
  "L0_000001.sst": NOT FOUND
  "L0_000004.sst": NOT FOUND
  "L1_000005.sst": NOT FOUND
  "L0_000003.sst": NOT FOUND
```

### Key Findings
1. ✅ Background flush triggers correctly (memory dropped from 88MB → 74MB)
2. ✅ SSTable files created on disk (3-5 files, 547KB-980KB each)
3. ❌ **key_0000000000 NOT FOUND in ANY SSTable** (written at start of test)
4. ❌ **ALL early keys (0-5) are MISSING**
5. ❌ **Data loss is COMPLETE** - not partial

## Impact

**User Impact**: CATASTROPHIC
- **Complete data loss** for all keys flushed by background worker
- Silent failure - no errors logged, writes appear successful
- Affects production workloads under memory pressure
- **Brand/company death** per user requirement

**Workload Impact**:
- Vector databases (high memory usage, frequent background flushes)
- High-throughput writes (triggers memory pressure)
- Any workload with `background_flush: true`

## Root Cause Analysis

### Initial Hypothesis (INCORRECT)
Thought the issue was:
1. Background flush clears `immutable_memtables` too early
2. Explicit `flush()` clears it during background flush
3. Race condition creates data loss window

### Fix Attempt #1 (FAILED)
Added wait logic in `flush()` to wait for background flush to complete before proceeding:
```rust
if self.options.background_flush {
    loop {
        let immut_arc = self.immutable_memtables.load();
        if immut_arc.is_none() {
            break; // Background flush completed
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
```

**Result**: Test still fails - keys still missing

### Actual Root Cause (CONFIRMED)
The problem is NOT with the timing or LSM tree updates. The problem is that **the background flush is writing SSTables that don't contain the data**.

**Evidence**:
- SSTables exist on disk with reasonable file sizes
- Manually opening SSTables confirms keys are NOT IN THE FILES
- This means the background flush worker is either:
  a) Building SSTables from empty memtables
  b) The memtable.iter() is not returning entries correctly
  c) The SSTableBuilder is not writing entries correctly

## Code Paths Involved

### 1. Memory Pressure Trigger
`src/db.rs:848-872` - `put()` checks memory pressure and calls `try_swap_memtable()`

### 2. Memtable Swap
`src/db.rs:1717-1754` - `try_swap_memtable()` swaps all 16 partitions:
```rust
for partition_mt in self.memtables.iter() {
    let old_arc: Arc<Memtable> =
        partition_mt.swap(Arc::new(Memtable::new(capacity_per_partition)));
    flushing_partitions.push(old_arc);
}
self.immutable_memtables.store(Arc::new(Some(Arc::new(flushing_partitions))));
```

### 3. Background Flush Worker
`src/background_workers.rs:89-231` - `run_background_flush_partitioned()`:
```rust
// Load immutable partitions
let immut_arc = immutable_memtables.load();
let immutable_partitions_arc = immut_arc.as_ref().as_ref().expect("...");

// Collect entries from ALL partitions
let mut all_entries: Vec<(Bytes, Entry)> = Vec::new();
for partition_mt in immutable_partitions_arc.iter() {
    for (key, entry) in partition_mt.iter() {  // ← Is this iterating correctly?
        all_entries.push((key, entry));
    }
}
all_entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

// Build SSTable
let mut builder = SSTableBuilder::create(&sstable_path)?;
for (key, entry) in &all_entries {  // ← Are entries being written?
    match entry {
        Entry::Value(value) => builder.add(key.clone(), value.clone())?,
        Entry::Tombstone => builder.add_tombstone(key.clone())?,
    }
}
builder.finish()?;  // ← Is finish() persisting correctly?
```

### 4. Explicit Flush
`src/db.rs:1387-1609` - `flush()` waits for background flush, then flushes active memtables

## Debugging Next Steps

1. **Add logging to background flush worker**:
   - Log number of entries collected from immutable partitions
   - Log first/last keys in collected entries
   - Log SSTable size before/after finish()

2. **Check memtable.iter() correctness**:
   - Verify it returns entries from immutable memtables
   - Check if Arc reference counting is preventing iteration

3. **Check SSTableBuilder**:
   - Verify entries are being written
   - Check if finish() is called correctly
   - Verify file fsync

4. **Verify partition_for_key()** :
   - Ensure keys go to expected partitions
   - Check if partitions are being skipped during iteration

## Temporary Workaround

**FOR TESTING ONLY** - Disable background flush:
```rust
let opts = DBOptions {
    background_flush: false,  // Disable background flush
    ..Default::default()
};
```

This forces synchronous flushes which appear to work correctly.

## Timeline

- **Nov 14, 2025 14:00**: Created comprehensive stress tests
- **Nov 14, 2025 14:30**: Discovered `test_memory_pressure_80_percent_trigger` fails
- **Nov 14, 2025 15:00**: Initial analysis - thought it was timing/race issue
- **Nov 14, 2025 15:30**: Attempted fix with wait logic - still fails
- **Nov 14, 2025 16:00**: **CRITICAL FINDING** - SSTables empty, data never written
- **Nov 14, 2025 16:30**: Investigation ongoing

## Related Issues

- Bug #7 (Compaction data loss) - Fixed via delayed deletion queue
- Bug #5 (Iterator invalidation) - Fixed via correct memtable collection order
- Bug #8 (WAL recovery race) - Fixed via barrier synchronization

## Test Case

`tests/stress_memory_pressure.rs::test_memory_pressure_80_percent_trigger`

**Configuration**:
- max_memory_bytes: 100MB
- memtable_capacity: 20MB
- background_flush: true
- Writes: 80,000 operations (1KB each)

**Expected**: All keys readable after flush
**Actual**: Early keys (0-50K) completely missing

---

**Status**: 🚨 Active investigation required before 0.0.1 release
**Priority**: **P0** - Blocks ALL production use
**Complexity**: **HIGH** - Deep background worker/LSM tree issue
**Risk**: **MAXIMUM** - Complete silent data loss

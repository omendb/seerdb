# Partitioned Memtables Implementation Plan

**Date**: November 7, 2025
**Status**: Work In Progress (struct updated, need to fix all call sites)
**Priority**: 3/6 in SOTA optimizations
**Expected Impact**: +25-40% write throughput (16x less lock contention)
**Complexity**: High (1-2 weeks)
**Research**: Tucana (2020), FASTER (2018)

---

## Overview

Replace single `Arc<Mutex<Memtable>>` with 16 hash-partitioned memtables `[Arc<Mutex<Memtable>>; 16]`.

**Key Benefit**: Concurrent writes to different partitions don't contend for the same lock.

---

## Changes Made ✅

### 1. Constants and Hash Function (Complete)
```rust
const NUM_PARTITIONS: usize = 16;

#[inline]
fn partition_for_key(key: &[u8]) -> usize {
    let mut hasher = XxHash64::default();
    key.hash(&mut hasher);
    let hash = hasher.finish();
    (hash % NUM_PARTITIONS as u64) as usize
}
```

### 2. DB Struct Updated (Complete)
```rust
pub struct DB {
    // OLD:
    // memtable: Arc<Mutex<Memtable>>,
    // immutable_memtable: Arc<Mutex<Option<Memtable>>>,

    // NEW:
    memtables: [Arc<Mutex<Memtable>>; NUM_PARTITIONS],
    immutable_memtables: Arc<Mutex<Option<Vec<Memtable>>>>,
    //...
}
```

---

## Remaining Work

### 3. DB::new() - Initialization
**Location**: `src/db.rs` line ~350

**Change**:
```rust
// OLD:
let memtable = Arc::new(Mutex::new(Memtable::new(options.memtable_capacity)));
let immutable_memtable = Arc::new(Mutex::new(None));

// NEW:
let memtables = std::array::from_fn(|_| {
    Arc::new(Mutex::new(Memtable::new(options.memtable_capacity / NUM_PARTITIONS)))
});
let immutable_memtables = Arc::new(Mutex::new(None));
```

**Note**: Each partition gets `capacity / NUM_PARTITIONS` to maintain total memory usage.

---

### 4. put() - Write to Correct Partition
**Location**: `src/db.rs` line ~730

**Change**:
```rust
// OLD:
let mt = self.memtable.lock().expect("Memtable lock poisoned");

// NEW:
let partition = partition_for_key(key);
let mt = self.memtables[partition].lock().expect("Memtable lock poisoned");
```

**Impact**: Only locks one partition, not all 16.

---

### 5. get() - Read from Correct Partition
**Location**: `src/db.rs` line ~818

**Change**:
```rust
// OLD:
let mt = self.memtable.lock().expect("Memtable lock poisoned");

// NEW:
let partition = partition_for_key(key);
let mt = self.memtables[partition].lock().expect("Memtable lock poisoned");
```

**Also need to check immutable_memtables**:
```rust
// Check all immutable partitions
let immut = self.immutable_memtables.lock().expect("Lock poisoned");
if let Some(ref partitions) = *immut {
    for partition_mt in partitions.iter() {
        if let Some(value) = partition_mt.get(key) {
            return Ok(value);
        }
    }
}
```

---

### 6. delete() - Delete from Correct Partition
**Location**: `src/db.rs` line ~1010

**Change**: Same as `put()` - use `partition_for_key()` to find correct partition.

---

### 7. flush() - Merge All Partitions
**Location**: `src/db.rs` line ~1100-1200

**This is the most complex change.**

**Current approach**: Flush single memtable to single SSTable.

**New approach**:
1. Swap all 16 partitions atomically
2. Collect entries from all partitions
3. Sort combined entries
4. Write to single SSTable

**Pseudocode**:
```rust
// 1. Swap all partitions
let mut new_memtables = Vec::with_capacity(NUM_PARTITIONS);
for i in 0..NUM_PARTITIONS {
    let mut guard = self.memtables[i].lock().expect("Lock poisoned");
    let old_mt = std::mem::replace(&mut *guard, Memtable::new(capacity_per_partition));
    new_memtables.push(old_mt);
}

// 2. Store as immutable
let mut immut_guard = self.immutable_memtables.lock().expect("Lock poisoned");
*immut_guard = Some(new_memtables);
drop(immut_guard);

// 3. Collect all entries from all partitions
let mut all_entries = Vec::new();
for partition_mt in pending_partitions {
    for entry in partition_mt.iter() {
        all_entries.push(entry);
    }
}

// 4. Sort by key (deduplication handled by SSTable)
all_entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

// 5. Build single SSTable from sorted entries
let mut builder = SSTableBuilder::new(...);
for (key, value) in all_entries {
    builder.add(key, value)?;
}
builder.finish(...)?;

// 6. Clear immutable partitions
let mut immut_guard = self.immutable_memtables.lock().expect("Lock poisoned");
*immut_guard = None;
```

**Complexity**: Medium-High (merging + sorting logic)

---

### 8. range() - Query All Partitions
**Location**: `src/db.rs` line ~900-950

**Current approach**: Query single memtable + SSTables.

**New approach**: Query all 16 partitions + SSTables.

**Change**:
```rust
// 1. Query all active memtable partitions
let mut partition_iterators = Vec::with_capacity(NUM_PARTITIONS);
for i in 0..NUM_PARTITIONS {
    let mt = self.memtables[i].lock().expect("Lock poisoned");
    let entries = mt.range(start, end);  // Collect entries in range
    partition_iterators.push(entries);
}

// 2. Query all immutable partitions
let immut = self.immutable_memtables.lock().expect("Lock poisoned");
if let Some(ref partitions) = *immut {
    for partition_mt in partitions.iter() {
        let entries = partition_mt.range(start, end);
        partition_iterators.push(entries);
    }
}

// 3. Merge all iterators with existing k-way merge
// Use existing RangeIterator / k-way merge infrastructure
```

**Note**: This already uses k-way merge, just need to add 16 more iterators.

---

### 9. Recovery - Load WAL into Partitions
**Location**: `src/db.rs` line ~400-500

**Change**:
```rust
// For each WAL record:
let partition = partition_for_key(&key);
let mut mt = db.memtables[partition].lock().expect("Lock poisoned");
mt.put(key, value);
```

**Impact**: WAL recovery distributes keys across partitions (same as normal writes).

---

### 10. Background Flush - Handle Partitions
**Location**: `src/db.rs` line ~1180-1250

**Change**: Background flush task needs to merge all partitions (same as sync flush).

---

## Testing Strategy

### Unit Tests
- [ ] Test `partition_for_key()` distribution (uniform hash)
- [ ] Test same key always goes to same partition (stable hash)

### Integration Tests
- [ ] All existing tests should pass (141 tests)
- [ ] Test concurrent writes to different partitions (no contention)
- [ ] Test concurrent writes to same partition (contention handled correctly)
- [ ] Test flush merges all partitions correctly
- [ ] Test range scan queries all partitions
- [ ] Test recovery loads into partitions

### Performance Tests
- [ ] Benchmark single-threaded writes (should be ~same speed)
- [ ] Benchmark multi-threaded writes (should be +25-40% faster)
- [ ] Measure lock contention reduction (profiling)

---

## Expected Results

### Before (Single Memtable)
- Lock contention: High on multi-core writes
- Throughput: 218K writes/sec (single-threaded baseline)
- Multi-core: Limited scalability due to lock contention

### After (16 Partitions)
- Lock contention: 16x lower (independent partition locks)
- Throughput: 218K writes/sec (single-threaded, same)
- Multi-core: +25-40% throughput (Tucana/FASTER papers)

---

## Risks and Mitigations

### Risk 1: Flush Complexity
**Issue**: Merging 16 partitions into 1 SSTable is complex
**Mitigation**: Use existing sort + dedup logic, test thoroughly

### Risk 2: Range Scan Performance
**Issue**: Querying 16 partitions might be slower
**Mitigation**: K-way merge already efficient, 16 iterators OK

### Risk 3: Memory Overhead
**Issue**: 16 memtables might use more memory
**Mitigation**: Divide capacity by 16, total memory same

### Risk 4: Small Key Overhead
**Issue**: Hash computation overhead for every operation
**Mitigation**: xxhash is extremely fast (~1-2ns), negligible

---

## Implementation Order

1. ✅ Add constants and hash function
2. ✅ Update DB struct
3. ⏳ Fix DB::new() initialization
4. ⏳ Fix put() operation
5. ⏳ Fix get() operation
6. ⏳ Fix delete() operation
7. ⏳ Fix flush() to merge partitions
8. ⏳ Fix range() to query all partitions
9. ⏳ Fix recovery
10. ⏳ Fix background flush
11. ⏳ Run all tests
12. ⏳ Benchmark performance

**Estimated Time**: 3-5 days of focused work

---

## References

- Tucana (Liu et al., 2020): Partitioned memtables for write-heavy workloads
- FASTER (Chandramouli et al., 2018): Lock-free concurrent structures
- SOTA_ALGORITHMIC_IMPROVEMENTS.md: Priority 3 details

---

**Status**: WIP - Struct updated, need to fix all call sites (16 compilation errors)
**Next Session**: Continue from step 3 (DB::new initialization)

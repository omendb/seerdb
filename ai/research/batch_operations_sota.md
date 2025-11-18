# Batch Operations in Storage Engines - SOTA Research

**Date**: November 17, 2025
**Focus**: Batch API patterns for amortizing overhead
**Scope**: RocksDB, LevelDB, BadgerDB, Pebble

---

## Key SOTA Patterns

### 1. RocksDB MultiGet() - Point Lookup Batching

**Pattern**: Batch multiple point lookups into single operation

```cpp
// Instead of:
for (key : keys) {
    values[key] = db->Get(key);  // N cache lookups, N index traversals
}

// RocksDB MultiGet:
db->MultiGet(keys, &values);  // Single pass, amortized overhead
```

**Benefits**:
- **3-5x faster** for batches of 10-100 keys
- Single index traversal
- Better cache locality
- Reduced lock contention

**Key insight**: Amortize fixed costs (cache lookup, index traversal, locking) across multiple operations

### 2. LevelDB/RocksDB WriteBatch - Write Batching

**Pattern**: Atomic batch writes with single WAL append

```cpp
WriteBatch batch;
batch.Put(key1, value1);
batch.Put(key2, value2);
batch.Delete(key3);
db->Write(batch);  // Single WAL write, atomic
```

**Benefits**:
- **10-15x faster** than individual writes
- Atomicity guarantee
- Single fsync
- Reduced WAL overhead

**Key insight**: Group sequential operations to reduce per-operation overhead

### 3. Badger WriteBatch - Deferred Execution

**Pattern**: Build batch, execute once

```go
wb := db.NewWriteBatch()
defer wb.Cancel()

for i := 0; i < N; i++ {
    wb.Set(key(i), value(i), 0)
}
wb.Flush()  // Execute all at once
```

**Benefits**:
- Deferred execution allows optimization
- Transaction batching
- Reduced contention

---

## Application to Prefix Scans

### Problem: omendb HNSW Workload

```rust
// Current: 18 separate iterator creations
for node in candidates {
    let neighbors = db.prefix(node_prefix)?;  // New iterator each time
    // Process neighbors
}
```

**Overhead per iteration**:
- Iterator creation
- Index block loading
- Cache warmup
- Memory allocation

**Total overhead**: 18× per query

### SOTA Solution: Batch Prefix API

```rust
// Batch approach:
let prefixes = candidates.map(|node| node_prefix);
let results = db.prefix_batch(&prefixes)?;  // Single iterator, sequential processing
```

**Expected benefits**:
- **3-5x faster** for batches of 10-20 prefixes
- Single iterator allocation
- Index block reuse across prefixes
- Better cache locality

---

## Design Decisions

### Sequential vs Concurrent

**Sequential** (Recommended):
```rust
pub fn prefix_batch(&self, prefixes: &[&[u8]]) -> Result<Vec<Vec<(Bytes, Bytes)>>>
```
- Process prefixes one by one
- Reuse single iterator state
- Simpler implementation
- **Best for graph traversal** (sequential access)

**Concurrent**:
```rust
pub fn prefix_batch_concurrent(&self, prefixes: &[&[u8]]) -> Result<Vec<Vec<(Bytes, Bytes)>>>
```
- Parallel processing
- Higher throughput
- More complex
- **Best for independent queries**

**Choice**: Sequential for now (omendb workload is sequential)

### Return Type Options

**Option A: Vec of Vec** (Recommended):
```rust
Vec<Vec<(Bytes, Bytes)>>  // One Vec per prefix
```
- Clear separation
- Easy to use
- Matches MultiGet pattern

**Option B: Flat Vec with markers**:
```rust
Vec<(usize, Bytes, Bytes)>  // (prefix_index, key, value)
```
- More efficient
- Harder to use
- Only if profiling shows Vec overhead

**Choice**: Option A (clarity over micro-optimization)

---

## Implementation Strategy

### Phase 1: Basic Sequential Batch

```rust
impl DB {
    pub fn prefix_batch(&self, prefixes: &[&[u8]]) -> Result<Vec<Vec<(Bytes, Bytes)>>> {
        let mut results = Vec::with_capacity(prefixes.len());

        // Single iterator creation overhead
        // Reuse cache/index state across prefixes
        for prefix in prefixes {
            let items: Vec<_> = self.prefix(prefix)?.collect()?;
            results.push(items);
        }

        Ok(results)
    }
}
```

**Optimization**: Share iterator state between prefix scans

### Phase 2: Optimized with Iterator Reuse

Key optimization: Don't create new iterator for each prefix
- Reuse SSTable iterators
- Reuse memtable references
- Amortize k-way merge setup

---

## Benchmarking Plan

**Workload**: omendb HNSW pattern (18 prefix scans)

```rust
// Baseline: Individual prefix scans
for prefix in prefixes {
    let results = db.prefix(prefix)?;
}

// Optimized: Batch prefix
let results = db.prefix_batch(&prefixes)?;
```

**Success criteria**:
- ✅ 3-5x faster for batch of 18 prefixes
- ✅ <200ms for omendb 10K query (vs current 1002ms)
- ✅ Reduced memory allocations
- ✅ Better cache hit rate

---

## References

**RocksDB**:
- MultiGet() - 3-5x improvement for batches
- WriteBatch - 10-15x improvement

**BadgerDB**:
- WriteBatch API with deferred execution
- Iterator reuse patterns

**LevelDB**:
- Batch interface for atomic operations
- Single WAL write optimization

---

## Decision

**Implement sequential batch prefix API** with iterator reuse optimization.

**Rationale**:
- General storage engine pattern (not vector-specific)
- Battle-tested approach (RocksDB MultiGet)
- Clear path to 3-5x improvement
- Solves omendb performance issue

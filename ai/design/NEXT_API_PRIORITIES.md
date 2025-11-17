# seerdb API: Priority Implementation Plan

**Status**: 0.0.1 Documentation phase
**Goal**: Define which missing features to implement in 0.0.2 and beyond
**Focus**: Impact vs. Complexity tradeoff

---

## Critical Path to Feature Parity

### Phase 0.0.1 (Current: Documentation & Release)
**Status**: Focus on stability, not new features
- ✅ Finalize existing API
- ✅ Complete documentation
- ✅ Performance benchmarks
- ✅ Clear roadmap for missing features

**Missing Features to Document**:
1. ❌ Range iterators
2. ❌ Snapshots/MVCC
3. ❌ Cloud storage (S3)
4. ❌ Transactions

---

## Phase 0.0.2 (High-Impact Features)

### PRIORITY 1: Range Iterators ⭐⭐⭐⭐⭐
**Impact**: CRITICAL (many use cases blocked)
**Complexity**: Medium
**Timeline**: 2-3 weeks
**Users Unblocked**: 70%

#### Why First
- Most common database operation after get/put
- Required for: time series, range queries, pagination, scanning
- Simple API, proven in RocksDB/fjall/sled
- No complex concurrency issues (readonly, snapshot at creation time)

#### Design

```rust
// Core iterator trait
pub trait Iterator: Send {
    fn key(&self) -> &[u8];
    fn value(&self) -> &[u8];
    fn next(&mut self) -> bool;  // Returns false when exhausted
    fn seek(&mut self, key: &[u8]);
    fn is_valid(&self) -> bool;
}

// Implement wrapper for easy use
pub struct RangeIterator { /* ... */ }

impl DB {
    /// Iterate all entries
    pub fn iter(&self) -> Result<RangeIterator> { }

    /// Range query: key_start..=key_end
    pub fn range<R: RangeBounds<[u8]>>(&self, bounds: R) -> Result<RangeIterator> { }

    /// Prefix search: keys starting with prefix
    pub fn prefix(&self, prefix: &[u8]) -> Result<RangeIterator> { }

    /// Reverse iteration
    pub fn iter_rev(&self) -> Result<ReverseIterator> { }
}

// Usage examples
for (key, value) in db.range(b"user_1000"..=b"user_1999")? {
    println!("{:?} = {:?}", key, value);
}

let mut iter = db.prefix(b"user_")?;
while let Some((key, value)) = iter.next()? {
    process_user(key, value);
}
```

#### Implementation Strategy
1. **Snapshot on creation**: Capture memtable + SSTable list
2. **Merge iteration**: Merge iterator over memtable + SSTables (like compaction merge)
3. **Seek support**: Binary search on ALEX index in SSTables
4. **Efficiency**:
   - Reuse existing `RangeMergeIterator` (already in codebase)
   - Use ALEX for O(log error) seeks
   - Lazy loading: don't materialize results

#### Backward Compatibility
- ✅ Zero breaking changes
- ✅ Pure addition to API
- ✅ Existing code continues to work

---

### PRIORITY 2: Snapshots ⭐⭐⭐⭐
**Impact**: HIGH (consistency semantics)
**Complexity**: Medium
**Timeline**: 2 weeks
**Users Unblocked**: 40%

#### Why Second
- Enables read consistency across multiple gets
- Required for: reporting, analytics, multi-row reads
- Current workaround: use batch API (limited to writes)
- Deferred from 0.0.1 but needed early in 0.0.2

#### Design

```rust
/// Point-in-time read view
///
/// All reads through this snapshot see the same consistent state,
/// even if other threads are writing during iteration.
pub struct Snapshot<'db> {
    // Captures: memtable version + SSTable list at creation time
}

impl DB {
    /// Create a read-only consistent snapshot
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            memtable_version: self.memtable.version(),
            sst_list: self.lsm_tree.levels().clone(),
        }
    }
}

impl<'db> Snapshot<'db> {
    /// Get with snapshot consistency
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> { }

    /// Range query on snapshot
    pub fn range<R: RangeBounds>(&self, bounds: R) -> Result<RangeIterator> { }
}

// Usage
let snapshot = db.snapshot();
let val1 = snapshot.get(b"key1")?;  // Consistent read
let val2 = snapshot.get(b"key2")?;  // Same snapshot version
drop(snapshot);  // Release resources
```

#### Implementation Strategy
1. **Snapshot structure**: Version + SSTable list
2. **Concurrent safety**:
   - Use ArcSwap for atomic SSTable list swap
   - Version numbers prevent use-after-free
3. **Efficiency**:
   - No copying (just pointer capture)
   - Auto-cleanup when dropped
   - Nested snapshots allowed

#### Backward Compatibility
- ✅ Zero breaking changes
- ✅ Optional (use with_snapshot or not)
- ✅ Existing code continues to work

---

### PRIORITY 3: Per-Operation ReadOptions ⭐⭐⭐
**Impact**: MEDIUM (tuning flexibility)
**Complexity**: Low
**Timeline**: 1 week
**Users Unblocked**: 20%

#### Why Third
- Enables advanced use cases: cache control, verification
- Simple to implement (orthogonal to snapshot)
- Low risk (just adds optional parameters)
- Proven pattern in RocksDB

#### Design

```rust
/// Options for read operations
#[derive(Clone)]
pub struct ReadOptions {
    /// Verify checksums on read (default: true)
    /// Set false for speed if you trust your hardware
    pub verify_checksums: bool,

    /// Fill block cache (default: true)
    /// Set false for one-off reads that won't be repeated
    pub fill_cache: bool,

    /// Maximum values to return (for bounded reads)
    pub limit: Option<usize>,
}

impl DB {
    /// Get with custom options
    pub fn get_with_options(&self, key: &[u8], opts: ReadOptions) -> Result<Option<Bytes>> { }
}

// Usage
let opts = ReadOptions {
    verify_checksums: false,  // Skip for speed
    fill_cache: false,        // Single-use read
    limit: None,
};
let val = db.get_with_options(b"key", opts)?;
```

#### Implementation Strategy
1. **Low coupling**: Pass ReadOptions to existing get path
2. **Block cache**: Respect fill_cache flag
3. **Checksum verification**: Skip if false
4. **Defaults**: Standard behavior when not specified

#### Backward Compatibility
- ✅ Zero breaking changes
- ✅ Optional parameter
- ✅ Defaults maintain current behavior

---

## Phase 0.0.3 (Advanced Features)

### PRIORITY 4: MVCC Transactions ⭐⭐⭐
**Impact**: HIGH (multi-row atomicity)
**Complexity**: High
**Timeline**: 4-6 weeks
**Users Unblocked**: 30%

#### Why Later
- Complex implementation (MVCC with version numbers)
- Deferred from 0.0.2 to focus on readers (snapshots/iterators)
- Can use batch API as workaround (write-only atomicity)
- Fewer users need this than snapshots/iterators

#### Design Preview

```rust
pub struct Transaction<'db> {
    // MVCC version + read set + write set
}

impl<'db> Transaction<'db> {
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> { }
    pub fn put(&mut self, key: &[u8], value: &[u8]) { }
    pub fn delete(&mut self, key: &[u8]) { }
    pub fn commit(self) -> Result<()> { }  // All-or-nothing
}

impl DB {
    pub fn transaction(&self) -> Result<Transaction> { }
}

// Usage: Transfer money between accounts
let mut txn = db.transaction()?;
let from_balance = txn.get(b"account:1000")?;
let to_balance = txn.get(b"account:2000")?;
txn.put(b"account:1000", b"50");   // Debit
txn.put(b"account:2000", b"150");  // Credit
txn.commit()?;  // All-or-nothing
```

---

### PRIORITY 5: S3 Backend (Cloud-Native) ⭐⭐⭐⭐
**Impact**: HIGH (deployment model)
**Complexity**: Medium
**Timeline**: 3-4 weeks
**Users Unblocked**: 50%

#### Why This Priority
- Enables cloud deployment (AWS, GCP, Azure)
- Hybrid model: local memtable + S3 SSTables
- Competitive advantage vs. RocksDB (local-only)
- Not as complex as full distributed system

#### Design Preview

```rust
use object_store::aws::AmazonS3Builder;

// Use S3 for SSTable storage
let store = AmazonS3Builder::from_env()
    .with_bucket_name("my-bucket")
    .build()?;

let opts = DBOptions {
    storage: Arc::new(store),
    ..Default::default()
};

let db = DB::open_with_storage(opts)?;

// Local memtable, S3 SSTables
db.put(b"key", b"value")?;  // Fast (local memtable)
db.flush()?;                // SSTables → S3
```

#### Implementation Strategy
1. **Abstract storage layer**: Use object_store trait
2. **Hybrid writes**: Memtable local, SSTables remote
3. **Durability**: WAL still local (safety-critical), SSTables on S3
4. **Multiple instances**: Share S3 bucket safely (timestamp-based filenames)

---

### PRIORITY 6: Merge Operators ⭐⭐
**Impact**: MEDIUM (specific use case)
**Complexity**: High
**Timeline**: 4 weeks
**Users Unblocked**: 15%

#### Use Cases
- Counters: `counter += 5` (not `put`)
- Aggregations: `sum_values.merge(new_value)`
- Time series: `values.append(timestamp, value)`

#### Design Preview

```rust
pub trait MergeOperator: Send + Sync {
    /// Merge a new partial value into existing value
    fn merge(&self, key: &[u8], existing: &[u8], partial: &[u8]) -> Bytes;
}

impl DB {
    pub fn merge(&self, key: &[u8], partial: &[u8]) -> Result<()> {
        // Get existing value → merge → put result
    }
}

// Usage: Counter increment
#[derive(Default)]
struct CounterMergeOp;

impl MergeOperator for CounterMergeOp {
    fn merge(&self, _key: &[u8], existing: &[u8], partial: &[u8]) -> Bytes {
        let a = u64::from_le_bytes(existing.try_into().unwrap_or([0; 8]));
        let b = u64::from_le_bytes(partial.try_into().unwrap_or([0; 8]));
        Bytes::from((a + b).to_le_bytes().to_vec())
    }
}

db.merge(b"counter:page_views", &(5u64).to_le_bytes())?;
```

#### Implementation Strategy
1. **Custom merge logic**: User-provided MergeOperator trait
2. **Background merging**: Compaction triggers merge ops
3. **Read-time merging**: Fallback if merge deferred

---

## Phase 0.0.4+ (Niche Features)

### PRIORITY 7: Watch/Subscribe ⭐⭐
**Impact**: LOW (niche pub-sub)
**Complexity**: Very High
**Timeline**: 6+ weeks
**Users Unblocked**: 5%

#### Only if Customer Demand
- Not core database feature
- Better solved by external event systems (Kafka, Redis pub-sub)
- High implementation complexity
- Defer until proven customer need

---

## Implementation Roadmap (Timeline)

```
0.0.1 (Current): Documentation + Release
  ├─ Finalize existing API
  ├─ Documentation completeness
  ├─ Performance validation
  └─ Roadmap definition ← YOU ARE HERE

0.0.2 (8-10 weeks): High-Impact Features
  ├─ Range Iterators (Priority 1) - 2-3 weeks
  ├─ Snapshots (Priority 2) - 2 weeks
  ├─ Per-Operation Options (Priority 3) - 1 week
  └─ Integration testing + stabilization

0.0.3 (12-16 weeks): Advanced Features
  ├─ MVCC Transactions (Priority 4) - 4-6 weeks
  ├─ S3 Backend (Priority 5) - 3-4 weeks
  └─ Integration + cloud testing

0.0.4+ (Future): Niche Features
  ├─ Merge Operators (Priority 6) - if needed
  ├─ Watch/Subscribe (Priority 7) - if demanded
  └─ Other customer-driven features
```

---

## Estimated Impact of Each Feature

### 0.0.1 (Current)
- **Potential Users**: Beta testers, research community
- **Blocking Issues**: Missing iterators, no cloud support
- **Workloads**: Benchmarking, proof-of-concept

### 0.0.2 (Iterators + Snapshots)
- **Potential Users**: Time series, analytics, reporting
- **Unblocks**: 70% of use cases
- **Feature Parity**: Comparable to fjall for reads

### 0.0.3 (Transactions + S3)
- **Potential Users**: Production deployments, cloud users
- **Unblocks**: 95% of use cases
- **Feature Parity**: Competitive with RocksDB

### 0.0.4+ (Advanced)
- **Potential Users**: Specialized applications
- **Niche Markets**: Counter apps, aggregation systems

---

## Test Plan for New Features

### Iterators (0.0.2)
```rust
#[test]
fn test_range_basic() {
    // Range query returns correct keys in order
}

#[test]
fn test_range_empty() {
    // Empty range returns no results
}

#[test]
fn test_prefix_match() {
    // Prefix returns all matching keys
}

#[test]
fn test_reverse_iteration() {
    // Reverse range works correctly
}

#[test]
fn test_seek() {
    // Seek to middle of range
}

#[test]
fn test_iterator_consistency() {
    // Iterator sees writes committed before creation only
}

#[test]
fn test_iterator_performance() {
    // Scan 1M entries in <100ms
}
```

### Snapshots (0.0.2)
```rust
#[test]
fn test_snapshot_consistency() {
    // Snapshot sees consistent state
}

#[test]
fn test_snapshot_isolation() {
    // Concurrent writes don't affect snapshot
}

#[test]
fn test_snapshot_release() {
    // Dropped snapshot releases resources
}

#[test]
fn test_multiple_snapshots() {
    // Multiple concurrent snapshots work
}
```

### S3 Backend (0.0.3)
```rust
#[test]
fn test_s3_put_get() {
    // S3 storage for SSTables works
}

#[test]
fn test_s3_multi_instance() {
    // Multiple instances share S3 bucket safely
}

#[test]
fn test_s3_recovery() {
    // Restart reads from S3 correctly
}

#[test]
fn test_hybrid_durability() {
    // WAL local, SSTables on S3
}
```

---

## Documentation Needs for 0.0.2

For each new feature, add:

1. **API documentation** (in code)
   - What it does
   - When to use it
   - Examples

2. **Guide** (in docs/)
   - Use cases
   - Performance tips
   - Common patterns

3. **Comparison** (in README)
   - How seerdb compares to RocksDB/fjall
   - Feature parity matrix
   - Migration guide

4. **Benchmarks**
   - Iterator performance
   - Snapshot overhead
   - Cache hit rates

---

## Risk Assessment

| Feature | Risk | Mitigation |
|---------|------|-----------|
| Range iterators | Low - proven pattern | Reuse RangeMergeIterator |
| Snapshots | Low - simple capture | ArcSwap provides safety |
| Transactions | High - complex MVCC | Start with simple MVCC, enhance later |
| S3 backend | Medium - cloud APIs | Use object_store trait (abstraction) |
| Merge operators | High - application semantics | Provide examples, templates |

---

## Conclusion

**Recommended Phase 0.0.2 Focus**: Iterators + Snapshots + ReadOptions
- **Unblocks**: 70% of users
- **Timeline**: 5 weeks (vs. 4-6 weeks for transactions alone)
- **Risk**: Low (proven patterns)
- **Impact**: Huge (feature parity with fjall/sled for reads)

**Recommended Phase 0.0.3 Focus**: Transactions + S3
- **Unblocks**: 95% of users
- **Timeline**: 8-10 weeks
- **Risk**: Medium
- **Impact**: Production-ready + cloud-deployable

**Not Recommended**: Watch/Subscribe, Merge operators in initial phases
- Niche features
- High complexity
- Wait for customer demand


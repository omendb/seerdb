# API Review - seerdb Batch API (Nov 8, 2025)

**Status**: 🚨 CRITICAL REVIEW REQUIRED 🚨
**Version**: 0.0.0 (pre-alpha, experimental)
**Stability**: UNSTABLE, UNTESTED IN PRODUCTION

---

## Executive Summary

**Assessment**: ⚠️ **NEEDS IMPROVEMENTS** before 0.0.1

**Critical Issues Found**:
1. ❌ **Atomicity NOT truly atomic** - WAL writes and memtable writes are separate (crash between them = partial state)
2. ⚠️ **Excessive cloning** - Every operation clones keys/values multiple times
3. ⚠️ **Missing rollback** - No way to undo batch if error occurs partway through
4. ⚠️ **Non-idiomatic API** - Differs from RocksDB/sled patterns in critical ways
5. ⚠️ **Minimal testing** - Only 3 basic tests, no edge cases, no failure scenarios

**Severity**: HIGH - This could cause data corruption or inconsistency

---

## 1. Atomicity Problem 🚨 **CRITICAL**

### Current Implementation

```rust
pub fn commit(self) -> Result<()> {
    // Step 1: Write to WAL
    for op in &self.operations {
        self.db.wal_tx.send(record)?;  // ← Can fail here
    }

    // Step 2: Apply to memtables
    for op in &self.operations {
        self.db.put_internal(key, value)?;  // ← Or here
    }

    Ok(())
}
```

### Problem

**NOT ATOMIC!** If the process crashes between WAL writes and memtable writes:
- WAL has operations recorded
- Memtables don't have data
- On recovery: WAL replays → data appears
- **BUT**: Reads before recovery return missing data!

### Example Failure Scenario

```rust
let mut batch = db.batch();
batch.put(b"account:1:balance", b"100");
batch.put(b"account:2:balance", b"50");
batch.commit()?;

// Crash happens after WAL write, before memtable write

// Before recovery:
assert_eq!(db.get(b"account:1:balance"), None);  // ❌ Data missing!

// After recovery:
assert_eq!(db.get(b"account:1:balance"), Some(b"100"));  // ✅ Data appears
```

### Impact

- **Data inconsistency** window between WAL write and memtable write
- **Not truly atomic** - violates user expectations
- **Silent data loss** - reads return None when data "should" exist

### Industry Standard (RocksDB)

RocksDB WriteBatch is truly atomic:
1. Builds batch in memory
2. Single atomic WAL write (all or nothing)
3. Single atomic memtable apply
4. Uses internal sequence numbers to ensure atomicity

### Fix Required

**Option 1**: Single WAL batch write (RECOMMENDED)
```rust
// Write entire batch to WAL as ONE operation
let batch_record = Record::Batch(self.operations.clone());
self.db.wal_tx.send(batch_record)?;

// Then apply to memtable
for op in &self.operations {
    self.db.put_internal(key, value)?;
}
```

**Option 2**: Two-phase commit
```rust
// Phase 1: Prepare (write to WAL)
let txn_id = self.db.prepare_batch(&self.operations)?;

// Phase 2: Commit (apply to memtable + mark WAL committed)
self.db.commit_batch(txn_id, &self.operations)?;
```

---

## 2. Performance Issues - Excessive Cloning

### Current Implementation

```rust
// Clone #1: User calls put()
batch.put(b"key", b"value");  // Bytes::copy_from_slice (clone)

// Clone #2: Convert to WAL Record
let record = Record::Put {
    key: key.clone(),      // Clone again!
    value: value.clone(),  // Clone again!
};

// Clone #3: put_internal
self.db.put_internal(key.clone(), value.clone())?;  // Clone AGAIN!
```

### Problem

**3x cloning** of every key and value:
- User data → Batch operations
- Batch operations → WAL records
- WAL records → Memtable

For 1000 operations with 1KB values = **3MB of unnecessary copies!**

### Industry Standard

RocksDB avoids cloning by using internal references or move semantics where possible.

### Fix Required

```rust
pub fn commit(self) -> Result<()> {
    // Move operations (no clone)
    for op in self.operations {  // ← Consume, don't borrow
        match op {
            Operation::Put { key, value } => {
                let record = Record::Put { key: key.clone(), value: value.clone() };
                self.db.wal_tx.send(record)?;
                self.db.put_internal(key, value)?;  // ← Move, no clone!
            }
            Operation::Delete { key } => {
                let record = Record::Delete { key: key.clone() };
                self.db.wal_tx.send(record)?;
                self.db.delete_internal(key)?;  // ← Move, no clone!
            }
        }
    }
    Ok(())
}
```

This reduces cloning from 3x to 1x (only WAL record clone).

---

## 3. Missing Rollback/Cleanup

### Current Implementation

```rust
pub fn commit(self) -> Result<()> {
    for op in &self.operations {
        self.db.wal_tx.send(record)?;  // If this fails partway...
    }

    for op in &self.operations {
        self.db.put_internal(key, value)?;  // ...we're in inconsistent state
    }

    Ok(())
}
```

### Problem

If WAL write #500 (out of 1000) fails:
- ✅ Operations 1-499 written to WAL
- ❌ Operations 500-1000 NOT written
- ❌ **No rollback** of 1-499
- Result: Partial batch committed (NOT ATOMIC!)

### Industry Standard

Databases either:
1. Write entire batch as single atomic operation (all or nothing)
2. Implement rollback to undo partial operations

### Fix Required

See "Atomicity Problem" fixes above.

---

## 4. API Design Issues

### Issue 4.1: Non-Standard Construction

**Our API**:
```rust
let batch = db.batch();  // Created from DB
batch.put(...);
batch.commit()?;
```

**RocksDB API**:
```rust
let mut batch = WriteBatch::default();  // Independent object
batch.put(...);
db.write(batch)?;  // DB writes the batch
```

**Impact**:
- ✅ Our API is more ergonomic (batch tied to DB)
- ❌ But less flexible (can't build batch independently)
- ⚠️ Different from industry standard

**Recommendation**: Keep our API (more ergonomic), but document difference

---

### Issue 4.2: Missing Builder Pattern

**Current**:
```rust
let mut batch = db.batch();
batch.put(b"k1", b"v1");
batch.put(b"k2", b"v2");
batch.commit()?;
```

**Industry Standard (Builder Pattern)**:
```rust
db.batch()
    .put(b"k1", b"v1")
    .put(b"k2", b"v2")
    .commit()?;
```

**Impact**:
- Our API requires `mut` binding
- Builder pattern is more fluent

**Fix**:
```rust
pub fn put(mut self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Self {
    let key = Bytes::copy_from_slice(key.as_ref());
    let value = Bytes::copy_from_slice(value.as_ref());
    self.operations.push(Operation::Put { key, value });
    self  // ← Return self
}
```

**Recommendation**: Support BOTH patterns (keep `&mut self` AND add builder variant)

---

### Issue 4.3: Missing Write Options

**Our API**:
```rust
batch.commit()?;  // No options
```

**RocksDB API**:
```rust
db.write_opt(batch, &WriteOptions::default())?;
```

**Missing Options**:
- Sync policy (fsync, fdatasync, none)
- Disable WAL for this batch
- Custom memtable selection

**Recommendation**: Add for 0.0.1
```rust
pub fn commit_with_options(self, opts: &WriteOptions) -> Result<()>
```

---

### Issue 4.4: No Batch Size Limits

**Current**: No limit on batch size

**Problem**:
```rust
let mut batch = db.batch();
for i in 0..10_000_000 {  // 10 million operations!
    batch.put(format!("key_{}", i), value);
}
batch.commit()?;  // OOM! Huge memory spike!
```

**Industry Standard**: Enforce size limits or warn

**Recommendation**: Add for 0.0.1
```rust
const MAX_BATCH_SIZE: usize = 10_000;  // Or make configurable

pub fn put(&mut self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()> {
    if self.operations.len() >= MAX_BATCH_SIZE {
        return Err(DBError::BatchTooLarge);
    }
    // ...
}
```

---

## 5. Testing Gaps 🚨 **CRITICAL**

### Current Tests (3 total)

1. `test_batch_basic` - Happy path (3 operations)
2. `test_batch_empty` - Empty batch
3. `test_batch_with_capacity` - 100 operations

### Missing Critical Tests

❌ **Atomicity tests**:
- Batch commit failure partway through
- Crash recovery after batch commit
- Concurrent reads during batch commit

❌ **Error handling tests**:
- WAL write failure
- Memtable full during batch
- Disk full during commit

❌ **Edge cases**:
- Duplicate keys in same batch
- Delete then put same key in batch
- Put then delete same key in batch
- Large batches (10K+ operations)
- Large keys/values (>1MB)

❌ **Concurrency tests**:
- Multiple concurrent batch commits
- Batch commit during compaction
- Batch commit during memtable flush

❌ **Correctness tests**:
- Batch operations visible atomically to readers
- Isolation (other readers don't see partial batch)
- Durability (batch survives crash after commit)

### Test Coverage Estimate

**Current**: ~5% coverage
**Required for 0.0.1**: >80% coverage

---

## 6. Documentation Issues

### Missing Documentation

❌ **Atomicity guarantees** - Not documented what "atomic" means
❌ **Failure semantics** - What happens on error?
❌ **Concurrency** - Thread-safe? Can multiple threads batch?
❌ **Performance characteristics** - When to use batch vs individual puts?
❌ **Size limits** - Is there a max batch size?
❌ **Memory usage** - How much memory does batch use?

### Existing Documentation Review

✅ **Good**: Examples are clear
✅ **Good**: Performance benefits mentioned
⚠️ **Missing**: Edge cases, failure modes, guarantees

---

## 7. Comparison with Industry Standards

### RocksDB WriteBatch

| Feature | RocksDB | seerdb | Status |
|---------|---------|--------|--------|
| **Atomicity** | ✅ True atomic | ❌ Not truly atomic | **CRITICAL** |
| **put()** | ✅ | ✅ | ✅ |
| **delete()** | ✅ | ✅ | ✅ |
| **clear()** | ✅ | ✅ | ✅ |
| **merge()** | ✅ | ❌ Missing | ⚠️ Nice-to-have |
| **delete_range()** | ✅ | ❌ Missing | ⚠️ Nice-to-have |
| **Independent construction** | ✅ | ❌ Tied to DB | ⚠️ Design choice |
| **Write options** | ✅ | ❌ Missing | ⚠️ Add for 0.0.1 |
| **Size limits** | ✅ Configurable | ❌ None | **CRITICAL** |
| **Rollback** | ✅ All-or-nothing | ❌ No rollback | **CRITICAL** |

### sled Batch

(sled uses a different pattern - direct Tree methods, not separate Batch object)

---

## 8. Risk Assessment

### HIGH RISK 🚨

1. **Data corruption**: Non-atomic commits can lead to inconsistent state
2. **OOM**: No batch size limits
3. **Silent failures**: Missing rollback on errors

### MEDIUM RISK ⚠️

1. **Performance**: Excessive cloning wastes CPU/memory
2. **API stability**: May need breaking changes for fixes
3. **Test coverage**: Low confidence in correctness

### LOW RISK ✅

1. **Compilation**: Code compiles, types are correct
2. **Basic functionality**: Simple cases work
3. **Documentation**: Examples are helpful

---

## 9. Recommendations for 0.0.1

### MUST FIX (Blockers)

1. ✅ **Implement true atomicity** (single WAL batch write OR two-phase commit)
2. ✅ **Add batch size limits** (prevent OOM)
3. ✅ **Comprehensive testing** (80%+ coverage)
4. ✅ **Document atomicity guarantees** (what happens on crash/error)

### SHOULD FIX (Important)

1. ⚠️ **Reduce cloning** (performance impact)
2. ⚠️ **Add write options** (flexibility)
3. ⚠️ **Add builder pattern** (ergonomics)

### NICE TO HAVE (Future)

1. 📅 **delete_range()** support
2. 📅 **merge()** operations
3. 📅 **Independent batch construction**

---

## 10. Idiomatic Rust Review

### Good Practices ✅

1. ✅ Lifetime parameters (`'db`) correct
2. ✅ Consuming `commit(self)` prevents reuse
3. ✅ `impl AsRef<[u8]>` for flexible keys/values
4. ✅ Clear error types with `Result<()>`
5. ✅ Doc comments with examples

### Non-Idiomatic Issues ❌

1. ❌ **Cloning in hot path** - Should use moves where possible
2. ❌ **No `Default` impl** - Builder pattern should support `Default::default()`
3. ❌ **Operations vec not `SmallVec`** - Small batches allocate unnecessarily
4. ❌ **No `#[must_use]`** on `batch()` - Easy to forget `.commit()`

### Suggested Improvements

```rust
use smallvec::SmallVec;

pub struct Batch<'db> {
    db: &'db DB,
    // Inline 8 operations (no heap allocation for small batches)
    operations: SmallVec<[Operation; 8]>,
}

#[must_use = "batch does nothing until commit() is called"]
pub fn batch(&self) -> Batch<'_> {
    Batch::new(self)
}
```

---

## 11. DX (Developer Experience) Review

### Positive DX ✅

1. ✅ Simple, intuitive API (`db.batch()` → `batch.put()` → `batch.commit()`)
2. ✅ Clear error messages
3. ✅ Good examples in docs
4. ✅ Follows Rust conventions (lifetime parameters, Result types)

### Negative DX ❌

1. ❌ **Silent non-atomicity** - Users expect atomic, don't get it
2. ❌ **No size limits** - Easy to OOM accidentally
3. ❌ **Missing failure documentation** - What happens on error?
4. ❌ **Requires `mut`** - Can't chain calls easily

### DX Score: 6/10

**Good basics, but critical gaps in atomicity and error handling**

---

## 12. Action Items for 0.0.1

### Priority 1 (Blockers) - DO NOT SHIP WITHOUT THESE

- [ ] Implement true atomicity (single WAL batch record)
- [ ] Add batch size limits (default 10K operations, configurable)
- [ ] Write comprehensive tests (atomicity, edge cases, failures)
- [ ] Document atomicity guarantees clearly
- [ ] Add rollback on partial failure

### Priority 2 (Important) - Ship with these if possible

- [ ] Reduce cloning (move semantics in commit())
- [ ] Add write options (sync policy, etc.)
- [ ] Add builder pattern variant
- [ ] Add `#[must_use]` attribute
- [ ] Use `SmallVec` for small batches

### Priority 3 (Nice to have) - Can defer to 0.0.2

- [ ] delete_range() support
- [ ] merge() operations
- [ ] Independent batch construction
- [ ] Batch iterator API

---

## 13. Final Verdict

**Current Status**: ❌ **NOT PRODUCTION READY**

**Blocker Issues**: 3 critical (atomicity, size limits, testing)

**Tasks for Production Readiness**:
- Fix atomicity issues
- Add size limits for batch operations
- Comprehensive testing
- API documentation

**Recommendation**:
1. ✅ DO NOT ship current batch API (not safe)
2. ✅ Fix critical issues before 0.0.1
3. ✅ Add comprehensive tests
4. ✅ Document all guarantees clearly

**For Now**:
- Mark batch API as `#[doc(hidden)]` or `#[deprecated]` until fixed
- OR clearly document it's experimental and NOT atomic yet
- Warn users NOT to rely on atomicity guarantees

---

## 14. Code Quality Checklist

- [ ] ✅ Compiles without warnings
- [ ] ❌ All tests pass (need more tests first!)
- [ ] ❌ Clippy clean (haven't run)
- [ ] ❌ rustfmt formatted (haven't run)
- [ ] ⚠️ Documentation complete (missing failure docs)
- [ ] ❌ Examples tested (docs examples not tested)
- [ ] ❌ Benchmarked (no batch-specific benchmarks)
- [ ] ❌ Fuzzed (no fuzzing tests)
- [ ] ❌ Sanitized (no MSAN/ASAN/TSAN runs)

**Quality Score**: 2/9 ❌

---

**Updated**: November 8, 2025
**Reviewer**: Claude (AI code review)
**Status**: 🚨 **NEEDS MAJOR IMPROVEMENTS** before 0.0.1

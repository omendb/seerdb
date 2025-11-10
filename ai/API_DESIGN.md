# seerdb API Design - Research & Recommendations

**Date**: November 9, 2025
**Status**: Planning phase (pre-0.0.1)
**Goal**: Stabilize API before 0.0.1 release

---

## Executive Summary

**Recommendation**: Keep simple, conventional storage engine API with minimal Rust ergonomics via traits.

**Rationale**:
- Simple API = easier to maintain, easier to bind to other languages
- Follow storage engine conventions (RocksDB, LevelDB, LMDB)
- Add Rust ergonomics through trait implementations, not new APIs
- Prioritize correctness and stability over cleverness

---

## Storage Engine API Conventions

### Industry Standard Pattern (RocksDB, LevelDB, LMDB, etc.)

All major storage engines follow this pattern:

```c
// Core operations
db_put(db, key, value)
db_get(db, key) -> value
db_delete(db, key)

// Batch operations
batch = db_batch_new()
db_batch_put(batch, key, value)
db_batch_delete(batch, key)
db_batch_commit(batch)

// Iteration
iter = db_iterator_new(db)
db_iterator_seek(iter, key)
db_iterator_next(iter)
db_iterator_value(iter) -> value
```

**Why this pattern?**:
1. Simple C ABI (easy to bind from any language)
2. Minimal state tracking
3. Clear ownership semantics
4. Battle-tested in production

### Current seerdb API

```rust
// Core operations
pub fn open(options: DBOptions) -> Result<Self>
pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()>
pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Bytes>>
pub fn delete(&self, key: impl AsRef<[u8]>) -> Result<()>

// Batch operations (PROBLEMATIC - see below)
pub fn batch(&self) -> WriteBatch
impl WriteBatch {
    pub fn put(self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Self
    pub fn delete(self, key: impl AsRef<[u8]>) -> Self
    pub fn commit(self) -> Result<()>
}

// Iteration
pub fn range(&self, range: impl RangeBounds<Vec<u8>>) -> RangeIterator
impl Iterator for RangeIterator {
    type Item = Result<(Bytes, Bytes)>;
    fn next(&mut self) -> Option<Self::Item>
}
```

**Assessment**:
- ✅ Core operations are simple and conventional
- ✅ Iterator follows Rust idioms
- ❌ Batch API has ownership issues (builder pattern with self-consuming methods)
- ⚠️ Missing: snapshot/transaction support
- ⚠️ Missing: explicit flush control

---

## Language Bindings Consideration

### C FFI Requirements

For Python, Go, Node.js, etc. bindings, we need a C-compatible API:

```rust
// C FFI layer (future work)
#[no_mangle]
pub extern "C" fn seerdb_open(path: *const c_char, options: *const c_void) -> *mut DB;

#[no_mangle]
pub extern "C" fn seerdb_put(db: *mut DB, key: *const u8, key_len: size_t, 
                               value: *const u8, value_len: size_t) -> c_int;
```

**Impact on API design**:
- Keep core API simple (maps easily to C FFI)
- Avoid complex Rust types (lifetimes, generics) in public API
- Builder patterns OK (map to option structs in C)

### Dual API Problems

**If we have two APIs** (simple + modern):
- 2x the FFI surface to maintain
- 2x the documentation
- 2x the testing
- Confusing for users ("which one should I use?")

**Conclusion**: Single API is better

---

## API Design Options

### Option 1: Current API (Minor Fixes) ✅ RECOMMENDED

Keep current API, fix the batch ownership issue:

```rust
// Core operations (UNCHANGED)
pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()>
pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Bytes>>
pub fn delete(&self, key: impl AsRef<[u8]>) -> Result<()>

// Batch operations (FIXED - borrow instead of consume)
pub fn batch(&self) -> WriteBatch
impl WriteBatch {
    pub fn put(&mut self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> &mut Self
    pub fn delete(&mut self, key: impl AsRef<[u8]>) -> &mut Self
    pub fn commit(self) -> Result<()>  // Consume on commit only
}

// Iteration (UNCHANGED)
pub fn range(&self, range: impl RangeBounds<Vec<u8>>) -> RangeIterator
```

**Usage**:
```rust
// Simple operations
db.put(b"key", b"value")?;
let value = db.get(b"key")?;

// Batch operations
let mut batch = db.batch();
batch.put(b"k1", b"v1")
     .put(b"k2", b"v2")
     .delete(b"k3");
batch.commit()?;

// Range scans
for entry in db.range(b"start"..b"end") {
    let (key, value) = entry?;
}
```

**Pros**:
- ✅ Minimal changes to existing code
- ✅ Simple, conventional API
- ✅ Easy to bind to C FFI
- ✅ Follows storage engine conventions

**Cons**:
- ❌ Not maximally "Rusty" (no type safety for keys/values)
- ❌ Manual error handling (no `?` in iterators without try blocks)

---

### Option 2: Typed API (Rejected - Too Complex)

```rust
pub trait Key: Serialize + Deserialize {}
pub trait Value: Serialize + Deserialize {}

pub struct TypedDB<K: Key, V: Value> {
    db: DB,
    _phantom: PhantomData<(K, V)>,
}

impl<K: Key, V: Value> TypedDB<K, V> {
    pub fn put(&self, key: &K, value: &V) -> Result<()>
    pub fn get(&self, key: &K) -> Result<Option<V>>
}
```

**Problems**:
- Complex generics don't map to C FFI
- Serialization adds overhead and complexity
- Most storage engines are byte-oriented (not typed)
- Doesn't match user expectations

---

### Option 3: Builder Pattern API (Rejected - Over-Engineered)

```rust
db.put()
    .key(b"key")
    .value(b"value")
    .execute()?;

db.batch()
    .put(b"k1", b"v1")
    .put(b"k2", b"v2")
    .commit()?;
```

**Problems**:
- More verbose than necessary
- Doesn't improve usability
- More code to maintain

---

## Recommended API (Option 1 with Extensions)

### Core API (Stable, Minimal Changes)

```rust
impl DB {
    // Open/close
    pub fn open(options: DBOptions) -> Result<Self>
    // Drop handles close automatically
    
    // Core operations (byte-oriented, zero-copy where possible)
    pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()>
    pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Bytes>>
    pub fn delete(&self, key: impl AsRef<[u8]>) -> Result<()>
    
    // Batch operations (fixed ownership)
    pub fn batch(&self) -> WriteBatch
    
    // Manual flush control
    pub fn flush(&self) -> Result<()>
    
    // Range iteration
    pub fn range(&self, range: impl RangeBounds<Vec<u8>>) -> RangeIterator
    
    // Statistics
    pub fn stats(&self) -> DBStats
}

impl WriteBatch {
    pub fn put(&mut self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> &mut Self
    pub fn delete(&mut self, key: impl AsRef<[u8]>) -> &mut Self
    pub fn commit(self) -> Result<()>  // Consumes batch
}

impl Iterator for RangeIterator {
    type Item = Result<(Bytes, Bytes)>;
    fn next(&mut self) -> Option<Self::Item>
}
```

### Ergonomic Extensions (Via Traits)

Add Rust ergonomics through trait implementations, not new APIs:

```rust
// String keys (convenience)
impl From<&str> for Key {
    fn from(s: &str) -> Self {
        Key(Bytes::from(s.as_bytes()))
    }
}

// Prefix iteration
impl DB {
    pub fn prefix(&self, prefix: impl AsRef<[u8]>) -> RangeIterator {
        // Syntactic sugar over range()
        let prefix = prefix.as_ref();
        let mut end = prefix.to_vec();
        // Increment last byte for exclusive upper bound
        if let Some(last) = end.last_mut() {
            *last = last.wrapping_add(1);
        }
        self.range(prefix.to_vec()..end)
    }
}

// IntoIterator for ergonomic usage
for entry in &db {  // Iterate all keys
    // ...
}
```

---

## Migration Plan

### Phase 1: Fix Critical API Issues (Before 0.0.1) 🚨

1. **Fix WriteBatch ownership** (src/batch.rs)
   - Change `put(self, ...)` → `put(&mut self, ...)`
   - Return `&mut Self` instead of `Self` for chaining
   - Only consume on `commit()`

2. **Add explicit close method** (optional)
   ```rust
   pub fn close(self) -> Result<()> {
       // Explicit close (Drop already handles this)
       // Useful for error handling
   }
   ```

3. **Document snapshot isolation semantics**
   - Clarify when iterators see which writes
   - Document transaction semantics (if any)

### Phase 2: Add Ergonomic Extensions (0.0.2+)

After API is stable and proven:

1. Add helper traits (From<&str>, etc.)
2. Add prefix() helper
3. Add IntoIterator for DB
4. Consider async API (separate module, opt-in feature)

### Phase 3: Language Bindings (0.1.0+)

After Rust API is stable:

1. C FFI layer
2. Python bindings (via PyO3 or C FFI)
3. Node.js bindings (via napi-rs)
4. Go bindings (via CGO)

---

## API Stability Guarantees

### 0.0.x Series (Pre-Alpha)
- API may change
- Deprecation warnings for breaking changes
- Migration guide for each release

### 0.1.x Series (Alpha)
- API mostly stable
- Only breaking changes if critical
- Semantic versioning starts here

### 1.0.0 (Stable)
- API frozen
- Only additions, no breaking changes
- Strong backward compatibility

---

## Comparison with Competitors

### RocksDB (C++)
```cpp
db->Put(key, value);
db->Get(key, &value);
db->Delete(key);

WriteBatch batch;
batch.Put(key1, value1);
batch.Put(key2, value2);
db->Write(WriteOptions(), &batch);
```

### sled (Rust)
```rust
db.insert(key, value)?;
db.get(key)?;
db.remove(key)?;

// No explicit batch API - all ops atomic
```

### LMDB (C)
```c
mdb_put(txn, dbi, &key, &data, 0);
mdb_get(txn, dbi, &key, &data);
mdb_del(txn, dbi, &key, NULL);
```

### seerdb (Proposed)
```rust
db.put(key, value)?;
db.get(key)?;
db.delete(key)?;

let mut batch = db.batch();
batch.put(key1, value1)
     .put(key2, value2);
batch.commit()?;
```

**Assessment**: seerdb API is conventional and familiar to storage engine users ✅

---

## Open Questions

### 1. Snapshot/Transaction Support?

**Current**: No explicit snapshots

**Options**:
- A) Add `db.snapshot()` → returns snapshot handle
- B) Iterators are implicit snapshots (current behavior)
- C) No snapshots (simpler, less flexible)

**Recommendation**: Option B (document current behavior)

### 2. Async API?

**Current**: Synchronous only

**Options**:
- A) Add async variants (`put_async`, `get_async`)
- B) Separate `AsyncDB` struct (opt-in feature)
- C) No async (storage engines are typically sync)

**Recommendation**: Option C for 0.0.1, revisit in 0.1.0

### 3. Key/Value Size Limits?

**Current**: Unlimited (controlled by memtable capacity)

**Options**:
- A) Document soft limits (e.g., max 4GB)
- B) Add explicit limits in DBOptions
- C) No limits (user responsibility)

**Recommendation**: Option A (document reasonable limits)

---

## Implementation Checklist

### Before 0.0.1 Release

- [ ] Fix WriteBatch ownership (change self → &mut self)
- [ ] Add WriteBatch tests (atomicity, error handling)
- [ ] Document API stability guarantees
- [ ] Document snapshot semantics for iterators
- [ ] Add API usage examples (5+ examples)
- [ ] Review all public APIs for consistency
- [ ] Add deprecation warnings for any changed APIs

### Before 0.1.0 Release

- [ ] Finalize API (no more breaking changes)
- [ ] Add ergonomic helper methods
- [ ] Add comprehensive API documentation
- [ ] Add API migration guide
- [ ] Consider C FFI layer design

---

## Conclusion

**Recommended API**: Keep simple, conventional storage engine API with minor fixes

**Key Principles**:
1. **Simplicity**: Easy to understand, easy to use, easy to bind
2. **Convention**: Follow storage engine patterns (RocksDB-like)
3. **Rust Ergonomics**: Add via traits, not new APIs
4. **Stability**: Freeze before 0.1.0, never break after 1.0.0

**Next Steps**:
1. Fix WriteBatch ownership issue
2. Continue bug fixes (block cache, etc.)
3. Stabilize and document API before 0.0.1

---

*This document will be updated as API evolves*

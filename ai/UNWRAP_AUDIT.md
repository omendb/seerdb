# Unwrap Audit - Production Code

**Last Updated**: November 1, 2025
**Status**: In Progress
**Total Unwraps**: 261 (library + tests + examples + benches)

---

## Strategy

### Priority Classification
1. **CRITICAL**: Production code in hot paths (put/get/delete)
2. **HIGH**: Production code in less frequent paths (flush, compaction, recovery)
3. **LOW**: Test code (acceptable to use unwrap in tests)
4. **IGNORE**: Examples and benchmarks (acceptable for demo code)

### Fix Approach
- **Mutex locks**: Create helper methods or use `.expect()` with context
- **Counter operations**: Use `.expect()` with descriptive messages
- **File operations**: Return proper errors via `?` operator
- **Test code**: Leave as-is (unwrap is acceptable in tests)

---

## Production Code Unwraps (src/)

### ✅ AUDIT COMPLETE - Summary

**Production Unwraps Fixed**: 17
- ✅ src/db.rs: 10 fixed
- ✅ src/sstable/mod.rs: 4 fixed
- ✅ src/wal/mod.rs: 3 fixed

**Test Unwraps (Acceptable)**: 244
- All remaining unwraps are in `#[cfg(test)]` sections
- Unwrap is acceptable and idiomatic in test code

**Total Progress**: 17/17 production unwraps fixed (100%)

---

### src/db.rs - 10 production unwraps ✅ FIXED

**CRITICAL** (hot paths - put/get/delete):
- ✅ Line 236: `self.wal.lock()` - WAL write in put()
- ✅ Line 260: `self.vlog.lock()` - vLog check in get()
- ✅ Line 263: `self.lsm.lock()` - LSM access in get()
- ✅ Line 292: `self.wal.lock()` - WAL write in delete()

**HIGH** (flush path):
- ✅ Line 311: `self.sstable_counter.lock()` - Counter in flush()
- ✅ Line 320: `self.vlog.lock()` - vLog access in flush()
- ✅ Line 349: `self.lsm.lock()` - LSM update in flush()

**HIGH** (compaction path):
- ✅ Line 385: `lsm.lock()` - LSM access in do_compact_level()
- ✅ Line 396: `sstable_counter.lock()` - Counter in do_compact_level()
- ✅ Line 411: `lsm.lock()` - LSM update in do_compact_level()

**Fix**: Replaced with `.expect("descriptive message")`
**Status**: ✅ Complete (commit b32fbf2)

---

### src/sstable/mod.rs - 4 production unwraps ✅ FIXED

**Location**: SSTable::open() (lines 248-251)
- ✅ Line 248: `footer_buf[0..8].try_into()` - index_offset parsing
- ✅ Line 249: `footer_buf[8..16].try_into()` - bloom_offset parsing
- ✅ Line 250: `footer_buf[16..20].try_into()` - checksum parsing
- ✅ Line 251: `footer_buf[20..24].try_into()` - version parsing

**Fix**: Replaced with `.expect("footer slice [...] is exactly N bytes")`
**Test Unwraps**: 54 (all in #[cfg(test)] section)
**Status**: ✅ Complete (commit pending)

---

### src/wal/mod.rs - 3 production unwraps ✅ FIXED

**Location**: WAL write methods
- ✅ Line 83: `self.file.lock()` - File lock in write()
- ✅ Line 103: `self.file.lock()` - File lock in write_batch()
- ✅ Line 135: `self.file.lock()` - File lock in sync()

**Fix**: Replaced with `.expect("WAL file mutex poisoned")`
**Test Unwraps**: 10 (all in #[cfg(test)] section)
**Status**: ✅ Complete (commit pending)

---

### Other Files - 0 production unwraps ✅

**src/vlog/mod.rs**: 0 production unwraps
- All 28 unwraps are in test code (#[cfg(test)])

**src/compaction/mod.rs**: 0 production unwraps
- All 22 unwraps are in test code (#[cfg(test)])

**src/compaction/merge.rs**: 0 production unwraps
- All 19 unwraps are in test code (#[test])

**src/wal/reader.rs**: 0 production unwraps
- All 11 unwraps are in test code (#[cfg(test)])

**src/memtable/mod.rs**: 0 production unwraps
- All 7 unwraps are in test code (#[cfg(test)])

**src/bloom/traditional.rs**: 0 production unwraps
- 1 unwrap in test code (#[test])

**src/bloom/bitpacked.rs**: 0 production unwraps
- 1 unwrap in test code (#[test])

**src/wal/record.rs**: 0 production unwraps
- All 2 unwraps are in test code (#[cfg(test)])

---

## Fix Plan

### Phase 1: Mutex Lock Helper (Priority: HIGH)
Create a helper trait or macro for `.lock().unwrap()` with better error handling:

```rust
// Option 1: Expect with context
self.wal.lock().expect("WAL mutex poisoned")

// Option 2: Helper trait
trait MutexExt<T> {
    fn lock_or_panic(&self, msg: &str) -> MutexGuard<T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_or_panic(&self, msg: &str) -> MutexGuard<T> {
        self.lock().unwrap_or_else(|e| {
            eprintln!("FATAL: {}: {}", msg, e);
            std::process::abort();
        })
    }
}
```

**Decision**: Use `.expect("descriptive message")` for now (simplest, clear intent)

### Phase 2: File Operations (Priority: MEDIUM)
Replace unwrap with `?` operator where possible:

```rust
// Before
let files = std::fs::read_dir(path).unwrap();

// After
let files = std::fs::read_dir(path)?;
```

### Phase 3: Audit Remaining Files (Priority: LOW)
Go through each file and categorize unwraps as production vs test code.

---

## Progress

### Completed ✅
- ✅ db.rs production unwraps (10/10) - commit b32fbf2
- ✅ sstable/mod.rs audit and fix (4/4) - commit pending
- ✅ wal/mod.rs audit and fix (3/3) - commit pending
- ✅ vlog/mod.rs audit (0 production unwraps)
- ✅ compaction/mod.rs audit (0 production unwraps)
- ✅ compaction/merge.rs audit (0 production unwraps)
- ✅ wal/reader.rs audit (0 production unwraps)
- ✅ wal/record.rs audit (0 production unwraps)
- ✅ memtable/mod.rs audit (0 production unwraps)
- ✅ bloom/traditional.rs audit (0 production unwraps)
- ✅ bloom/bitpacked.rs audit (0 production unwraps)

### Metrics
- **Production unwraps fixed: 17/17 (100%)**
- **Test unwraps (left as-is): 244**
- **Total progress: 100% ✅**

### Breakdown
- **db.rs**: 10 mutex locks → `.expect("mutex poisoned")`
- **sstable/mod.rs**: 4 array conversions → `.expect("slice is exactly N bytes")`
- **wal/mod.rs**: 3 mutex locks → `.expect("WAL file mutex poisoned")`

---

## Notes

**Mutex Poisoning**:
- Occurs when a thread panics while holding a lock
- In production, we generally want to abort on poisoned mutex
- Using `.expect()` with descriptive message is acceptable
- Alternative: Use `.unwrap_or_else(|e| { abort })` for custom handling

**Test Code**:
- Unwrap is acceptable and idiomatic in tests
- Makes tests cleaner and easier to read
- Test failures show stack trace anyway

**Performance**:
- `.expect()` has same performance as `.unwrap()`
- No runtime cost for better error messages
- Only downside: slightly larger binary (negligible)

---

*Last Updated: November 1, 2025*
*Owner: Production hardening team*

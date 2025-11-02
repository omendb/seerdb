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

### src/db.rs - 13 production unwraps

**CRITICAL** (hot paths - put/get/delete):
- Line 236: `self.wal.lock().unwrap()` - WAL write in put()
- Line 260: `self.vlog.lock().unwrap()` - vLog check in get()
- Line 263: `self.lsm.lock().unwrap()` - LSM access in get()
- Line 292: `self.wal.lock().unwrap()` - WAL write in delete()

**HIGH** (flush path):
- Line 311: `self.sstable_counter.lock().unwrap()` - Counter in flush()
- Line 320: `self.vlog.lock().unwrap()` - vLog access in flush()
- Line 349: `self.lsm.lock().unwrap()` - LSM update in flush()

**HIGH** (compaction path):
- Line 385: `lsm.lock().unwrap()` - LSM access in do_compact_level()
- Line 396: `sstable_counter.lock().unwrap()` - Counter in do_compact_level()
- Line 411: `lsm.lock().unwrap()` - LSM update in do_compact_level()

**Status**: 10 production unwraps need fixing

---

### Other Files (Need Audit)

**src/vlog/mod.rs**: 28 unwraps
- Need to audit which are production vs test code

**src/sstable/mod.rs**: 58 unwraps
- Need to audit which are production vs test code

**src/compaction/mod.rs**: 22 unwraps
- Need to audit which are production vs test code

**src/compaction/merge.rs**: 19 unwraps
- Need to audit which are production vs test code

**src/wal/reader.rs**: 11 unwraps
- Need to audit which are production vs test code

**src/wal/mod.rs**: 13 unwraps
- Need to audit which are production vs test code

**src/memtable/mod.rs**: 7 unwraps
- Need to audit which are production vs test code

**src/bloom/traditional.rs**: 1 unwrap
**src/bloom/bitpacked.rs**: 1 unwrap
**src/wal/record.rs**: 2 unwraps

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

### Completed
- [ ] db.rs production unwraps (0/10)
- [ ] vlog/mod.rs audit
- [ ] sstable/mod.rs audit
- [ ] compaction/mod.rs audit
- [ ] compaction/merge.rs audit
- [ ] wal/reader.rs audit
- [ ] wal/mod.rs audit
- [ ] memtable/mod.rs audit

### Metrics
- Production unwraps fixed: 0
- Test unwraps (left as-is): TBD
- Total progress: 0%

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

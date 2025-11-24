# TODO - seerdb

**Last Updated**: November 24, 2025
**Focus**: Omendb Integration (MVCC completion)
**Version**: 0.0.1-alpha

---

## Active Tasks

### 1. MVCC Completion (High Priority)

| Task | Status | Notes |
|------|--------|-------|
| InternalKey type | ✅ Done | `src/types.rs` |
| Memtable MVCC | ✅ Done | Sorted by (key ASC, seq DESC) |
| WAL versioning | ✅ Done | Records include seq numbers |
| DB seq assignment | ✅ Done | `put()`, `delete()`, `merge()` |
| Snapshot API | ✅ Done | `db.snapshot()` works |
| SSTable MVCC methods | ✅ Done | `add_internal()`, `get_mvcc()` |
| **DB flush MVCC** | 🚧 TODO | `flush()` needs to use `add_internal()` |
| **MVCC garbage collection** | ❌ TODO | Old versions accumulate |

### 2. Bug Fixes

| Task | Status | Notes |
|------|--------|-------|
| Fix `test_corrupted_wal_detected` | ✅ Done | Already fixed in previous WAL format changes |

### 3. Transaction API (Medium Priority)

| Task | Status | Notes |
|------|--------|-------|
| `db.begin_transaction()` | ❌ TODO | Returns `Transaction` handle |
| `Transaction.get/put/delete` | ❌ TODO | Buffered writes |
| `Transaction.commit()` | ❌ TODO | Atomic batch + OCC validation |
| `Transaction.abort()` | ❌ TODO | Discard buffer |

---

## Backlog (Deferred)

| Task | Priority | Trigger |
|------|----------|---------|
| Async I/O (io_uring) | Low | When syscall overhead is bottleneck |
| Cloud storage hardening | Low | Production scaling phase |
| Compaction tuning | Low | Write stall reports |

---

## Completed (Nov 2025)

- ✅ SSTable MVCC methods (`add_internal()`, `get_mvcc()`)
- ✅ MVCC core types and memtable refactor
- ✅ WAL reader/writer compatibility (length prefixes)
- ✅ Snapshot API with sequence number isolation
- ✅ Reverse iteration (`iter_rev()`, `range_rev()`)
- ✅ Range iterators (`range()`, `prefix()`)
- ✅ Merge operators (`db.merge()`)
- ✅ Fixed flaky crash recovery test
- ✅ ai/ directory cleanup (68 → 15 files)
- ✅ Documentation audit and update

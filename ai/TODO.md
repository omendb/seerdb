# TODO - seerdb

**Last Updated**: November 24, 2025
**Focus**: Transaction API
**Version**: 0.0.1-alpha

---

## Active Tasks

### 1. Transaction API (Medium Priority)

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

- ✅ MVCC core (InternalKey, Memtable, WAL, SSTable methods)
- ✅ **DB flush MVCC** - Flush/compaction now preserve all MVCC versions
- ✅ **MVCC GC** - SnapshotTracker + GC-aware compaction
- ✅ Snapshot API, range/reverse iterators, merge operators
- ✅ Crash recovery tests fixed
- ✅ ai/ cleanup (68 → 16 files)

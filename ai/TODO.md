# TODO - seerdb

**Last Updated**: November 25, 2025
**Focus**: Stable - ready for oadb integration
**Version**: 0.0.1-alpha

---

## Active Tasks

None - Transaction API complete with tests and benchmarks.

---

## Test Coverage

| Area | Unit | Integration | Stress | Bench |
|------|------|-------------|--------|-------|
| Core DB | ✅ | ✅ | ✅ | ✅ |
| Batch writes | ✅ | ✅ | ✅ | ✅ |
| Snapshots | ✅ | ✅ | ✅ | - |
| **Transaction API** | ✅ 7 | ✅ 9 | ✅ | ✅ |
| Compaction | ✅ | ✅ | ✅ | ✅ |
| Crash recovery | ✅ | ✅ | ✅ | ✅ |

---

## Backlog (Deferred)

| Task | Priority | Trigger |
|------|----------|---------|
| Lock-free OCC | Low | When commit lock becomes bottleneck |
| Async I/O (io_uring) | Low | When syscall overhead is bottleneck |
| Cloud storage hardening | Low | Production scaling phase |
| Compaction tuning | Low | Write stall reports |
| SSI (Serializable Snapshot Isolation) | Low | If SI anomalies become an issue |

---

## Completed (Nov 2025)

- ✅ **Transaction benchmark** - 52K txn/sec, ~0% overhead vs raw put
- ✅ **Transaction OCC bug fix** - Added commit lock to prevent TOCTOU race
- ✅ **Transaction integration tests** - 9 tests covering concurrency, crash recovery, snapshots
- ✅ **Transaction API** - OCC with snapshot isolation, read-set conflict detection
- ✅ MVCC core (InternalKey, Memtable, WAL, SSTable methods)
- ✅ **DB flush MVCC** - Flush/compaction now preserve all MVCC versions
- ✅ **MVCC GC** - SnapshotTracker + GC-aware compaction
- ✅ Snapshot API, range/reverse iterators, merge operators
- ✅ Crash recovery tests fixed
- ✅ ai/ cleanup (68 → 16 files)

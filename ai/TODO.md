# TODO - seerdb

**Last Updated**: November 25, 2025
**Focus**: Transaction API stabilization
**Version**: 0.0.1-alpha

---

## Active Tasks

### Transaction API Testing (P0 - Blocking oadb release)

| Task | Status | Notes |
|------|--------|-------|
| Concurrent transaction conflict test | ❌ TODO | Multiple threads, same keys, OCC validation |
| Transaction crash recovery test | ❌ TODO | Committed txn survives crash |
| Transaction + snapshot interaction test | ❌ TODO | Txn reads from snapshot, concurrent writes |

### Transaction API Testing (P1 - Should have)

| Task | Status | Notes |
|------|--------|-------|
| Transaction benchmark | ❌ TODO | Throughput measurement |
| Large transaction stress test | ❌ TODO | 10K+ keys in read-set/write-buffer |
| Mixed workload test | ❌ TODO | txn + raw put/get concurrently |

---

## Test Coverage Gaps

| Area | Unit | Integration | Bench | Stress |
|------|------|-------------|-------|--------|
| Core DB | ✅ | ✅ | ✅ | ✅ |
| Batch writes | ✅ | ✅ | ✅ | ✅ |
| Snapshots | ✅ | ✅ | ❌ | ✅ |
| **Transaction API** | ✅ 7 | ❌ | ❌ | ❌ |
| Compaction | ✅ | ✅ | ✅ | ✅ |
| Crash recovery | ✅ | ✅ | ✅ | ✅ |

---

## Backlog (Deferred)

| Task | Priority | Trigger |
|------|----------|---------|
| Async I/O (io_uring) | Low | When syscall overhead is bottleneck |
| Cloud storage hardening | Low | Production scaling phase |
| Compaction tuning | Low | Write stall reports |
| SSI (Serializable Snapshot Isolation) | Low | If SI anomalies become an issue |

---

## Completed (Nov 2025)

- ✅ **Transaction API** - OCC with snapshot isolation, read-set conflict detection
- ✅ MVCC core (InternalKey, Memtable, WAL, SSTable methods)
- ✅ **DB flush MVCC** - Flush/compaction now preserve all MVCC versions
- ✅ **MVCC GC** - SnapshotTracker + GC-aware compaction
- ✅ Snapshot API, range/reverse iterators, merge operators
- ✅ Crash recovery tests fixed
- ✅ ai/ cleanup (68 → 16 files)

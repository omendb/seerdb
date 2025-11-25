# TODO - seerdb

**Last Updated**: November 25, 2025
**Focus**: Ready for omendb integration
**Version**: 0.0.1-alpha

---

## Active Tasks

None - Transaction API complete. Ready for omendb integration.

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

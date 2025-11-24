# TODO - seerdb

**Last Updated**: November 24, 2025
**Current Focus**: Omendb Development Support (MVCC)
**Version**: 0.0.1-alpha (Stable Checkpoint)
**Status**: Active Development

---

## Active Tasks (Omendb Priorities)

### 1. MVCC Transactions (Snapshot Isolation)
- [x] **Core Types**: Create `src/types.rs` for `InternalKey` (UserKey + SeqNum + Type).
- [x] **Memtable Refactor**: Update `Memtable` to store `InternalKey` (sorted by Key ASC, Seq DESC).
- [x] **WAL Versioning**: Update `Record` to include sequence numbers.
- [x] **DB Integration**: Refactor `DB::put`/`get`/`delete`/`merge` to assign sequence numbers.
- [ ] **SSTable Lookup**: Update `SSTable::get()` to use InternalKey for MVCC-aware lookups.
- [ ] **Snapshot API**: Implement `db.snapshot()` returning read-only view at sequence number.
- [ ] **Transaction API**: Implement `db.begin_transaction()` and `Transaction` struct.

---

## Backlog (Deferred)

### Performance
- [ ] **Async I/O (io_uring)**: Investigate replacing `std::fs` (Defer until needed).
- [ ] **Vector Index**: Integrate `seerdb-vector` (Proprietary/Omendb side).

---

## Completed Tasks
- ✅ MVCC core types and memtable refactor (Nov 24, 2025)
- ✅ WAL reader/writer compatibility fix (Nov 24, 2025)
- ✅ DB sequence number assignment (Nov 24, 2025)
- ✅ Integration tests updated for MVCC API (Nov 24, 2025)
- ✅ Fixed flaky crash recovery test (Nov 24, 2025)

# TODO - seerdb

**Last Updated**: November 23, 2025
**Current Focus**: Omendb Development Support (MVCC)
**Version**: 0.0.1-alpha (Stable Checkpoint)
**Status**: 202+ tests passing. Release Halted for Dev.

---

## Active Tasks (Omendb Priorities)

### 1. MVCC Transactions
- [ ] **API Design**: Define `Transaction` struct and `db.begin_transaction()`.
- [ ] **Write Buffer**: Implement local mutation buffer in `Transaction`.
- [ ] **Conflict Detection**: Implement OCC (Optimistic Concurrency Control) validation on commit.
- [ ] **Integration**: Wire up to `Batch` commit.

---

## Backlog (Deferred)

### Performance
- [ ] **Async I/O (io_uring)**: Investigate replacing `std::fs` (Defer until needed).
- [ ] **Vector Index**: Integrate `seerdb-vector` (Proprietary/Omendb side).

---

## Completed Tasks

### Omendb Integration (Nov 2025)
- [x] **Omendb Benchmark**: `benches/omendb_simulation.rs` created.
  - Validated `MergeOperator` throughput (230K ops/sec, equal to raw Put).
- [x] **Reverse Iteration**: Implemented `iter_rev()` and `range_rev()`.
  - Added `DoubleEndedIterator` support to Memtable, Block, and SSTable.
  - Implemented `KWayMergeIteratorRev` (Max-Heap merge).
  - Added `DB::iter_rev()`, `DB::range_rev()`.

### Release v0.0.1-alpha (Nov 2025)
- [x] **Snapshot Isolation**: Fixed race condition with concurrent compaction.
- [x] **SOTA Verification**: Confirmed 4.65M reads/sec on Linux.
- [x] **Tag**: `v0.0.1-alpha` created.

### Core Features
- [x] **Merge Operators**: O(1) blind writes for graphs.
- [x] **Prefix Bloom Filters**: Optimized for graph scans.
- [x] **Durability**: WAL + fsync.
- [x] **LeanStore**: Buffer Pool implemented.

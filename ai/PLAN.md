# PLAN - seerdb

**Goal**: Storage engine for `omendb` (Vector/Graph database)
**Status**: Active Development
**Last Updated**: November 24, 2025

---

## Current Phase: MVCC Completion

### ✅ Completed

| Feature | Status | Notes |
|---------|--------|-------|
| InternalKey (MVCC core) | ✅ | `src/types.rs` |
| Memtable MVCC | ✅ | Sorted by (key ASC, seq DESC) |
| WAL versioning | ✅ | Records include seq numbers |
| Snapshot API | ✅ | `db.snapshot()` with seq isolation |
| Reverse iteration | ✅ | `iter_rev()`, `range_rev()` |
| Range iterators | ✅ | `range()`, `prefix()`, `prefix_batch()` |
| Merge operators | ✅ | `db.merge()` for graph edges |

### 🚧 In Progress

| Feature | Priority | Notes |
|---------|----------|-------|
| SSTable MVCC lookup | High | `get()` needs InternalKey |
| MVCC garbage collection | Medium | Old versions accumulate |

### ❌ Next Up

| Feature | Priority | Notes |
|---------|----------|-------|
| Transaction API | Medium | `begin_transaction()` / `commit()` |

---

## Roadmap

### Phase 1: MVCC Foundation ✅ (Nov 2025)
- [x] InternalKey type
- [x] Memtable MVCC
- [x] WAL versioning
- [x] Snapshot reads

### Phase 2: MVCC Completion (Current)
- [ ] SSTable MVCC-aware lookups
- [ ] Version garbage collection
- [ ] Transaction API (OCC)

### Phase 3: Omendb Integration
- [ ] Performance validation on graph workloads
- [ ] Prefix scan optimization for edge lists
- [ ] Integration with `seerdb-vector`

---

## Architecture Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| MVCC | Native InternalKey | RocksDB-style, no external deps |
| Compaction | Leveled | Bounded read amp for graph queries |
| Value separation | WiscKey vLog | 4.82x better write amp |
| Learned index | ALEX | Faster SSTable block lookups |
| Buffer management | Software pool | LeanStore-lite approach |

---

## Deferred (Post-Integration)

| Feature | Trigger |
|---------|---------|
| Async I/O (io_uring) | Syscall overhead bottleneck |
| Cloud hardening | Production scaling |
| Column families | User demand (use key prefixes) |

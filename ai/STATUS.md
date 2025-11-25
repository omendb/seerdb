# STATUS - seerdb

**Last Updated**: November 24, 2025
**Version**: 0.0.1-alpha
**Status**: Active Development (Omendb Integration)

---

## Current State

| Metric | Value |
|--------|-------|
| **Tests** | 200 lib + integration tests passing |
| **Compilation** | Clean (no errors, no warnings) |
| **Lines of Code** | ~30K Rust |
| **ai/ Files** | 16 (cleaned from 68) |

## Feature Implementation Status

### ✅ Implemented (Working)

| Feature | Location | Notes |
|---------|----------|-------|
| **MVCC InternalKey** | `src/types.rs` | UserKey + SeqNum + ValueType |
| **Memtable MVCC** | `src/memtable/mod.rs` | Sorted by (key ASC, seq DESC) |
| **WAL Versioning** | `src/wal/record.rs` | Records include sequence numbers |
| **SSTable MVCC** | `src/sstable/mod.rs` | `add_internal()`, `get_mvcc()` methods |
| **Snapshot API** | `src/snapshot.rs`, `db.snapshot()` | Read isolation at sequence number |
| **Range Iterators** | `db.range()`, `db.iter()` | Forward iteration with merge |
| **Reverse Iteration** | `db.iter_rev()`, `db.range_rev()` | Backward iteration |
| **Prefix Scans** | `db.prefix()`, `db.prefix_batch()` | Efficient prefix queries |
| **Merge Operators** | `src/merge_operator.rs`, `db.merge()` | Custom merge functions |
| **Batch Writes** | `src/batch.rs`, `db.batch()` | Atomic multi-key writes |
| **WiscKey vLog** | `src/vlog/` | Value separation for large values |
| **Background Compaction** | `src/background_workers.rs` | Async compaction |
| **Bloom Filters** | `src/bloom/`, SSTable | Traditional + prefix bloom |
| **ALEX Learned Index** | `src/alex/`, SSTable | Faster block lookups |
| **Buffer Pool** | `src/buffer/` | Page caching with eviction |
| **Health Checks** | `src/health.rs` | Database health monitoring |
| **Object Store** | Feature flag `object-store` | S3/GCS backend support |

### 🚧 Partial / Needs Work

| Feature | Status | Gap |
|---------|--------|-----|
| **DB flush MVCC** | ✅ Done | Flush/compaction write MVCC entries |
| **MVCC GC** | ✅ Done | SnapshotTracker tracks active snapshots, compaction GC's old versions |

### ❌ Not Implemented

| Feature | Priority | Notes |
|---------|----------|-------|
| **Transaction API** | Medium | `begin_transaction()` / `commit()` / `abort()` |
| **Column Families** | None | Use key prefixes instead |

---

## Performance Baseline (v0.0.1-alpha)

| Workload | Performance | vs RocksDB |
|----------|-------------|------------|
| **Writes** | 878K ops/sec | 2.47x faster |
| **Reads** | 4.65M ops/sec | 2.07x faster |
| **Graph Merge** | 230K ops/sec | - |
| **Write Amp** | 1.01x (with vLog) | 4.82x better |

---

## Architecture

```
Write Path:  put() → WAL (seq#) → Memtable (InternalKey) → [flush] → SSTable
Read Path:   get() → Memtable → Immutable Memtables → L0..L6 SSTables
Snapshot:    Captures current seq# → reads filter by seq ≤ snapshot_seq
```

### Module Map

```
src/
├── types.rs          # InternalKey, ValueType (MVCC core)
├── db.rs             # Main DB interface (~200K lines)
├── memtable/         # Partitioned concurrent skiplist
├── wal/              # Write-ahead log with seq numbers
├── sstable/          # SSTable format with bloom + ALEX
├── compaction/       # Leveled compaction + filters
├── vlog/             # WiscKey value separation
├── snapshot.rs       # Point-in-time reads
├── range.rs          # Range iteration
├── batch.rs          # Atomic batch writes
├── bloom/            # Traditional + learned bloom
├── alex/             # ALEX learned index
└── buffer/           # Buffer pool management
```

---

## Recent Changes (Nov 24, 2025)

1. **SSTable MVCC Methods**: `add_internal()`, `get_mvcc()` for version-aware read/write
2. **MVCC Core Complete**: Sequence numbers flow through entire write path
3. **MVCC Garbage Collection**: SnapshotTracker + GC-aware compaction
4. **DB flush MVCC**: Flush/compaction preserve all MVCC versions
5. **Cleaned ai/ Directory**: 68 → 15 files

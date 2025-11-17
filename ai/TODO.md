# TODO - seerdb

**Last Updated**: November 16, 2025
**Current Sprint**: Feature Completeness (Snapshots + Convenience APIs)
**Previous**: Feature audit revealed range queries WORK, snapshots missing
**Timeline**: 4-6 weeks to 0.0.1

---

## ✅ COMPLETED: Snapshots (Nov 16, 2025)

**Commit**: `1ecaace` - feat: implement point-in-time snapshots

**Implementation**:
- ✅ `db.snapshot()` - Lightweight snapshot (SSTable data only)
- ✅ `db.snapshot_consistent()` - Full consistency (forces flush first)
- ✅ `snapshot.get(key)` - Point lookup from snapshot
- ✅ `snapshot.range(start, end)` - Range queries on snapshot
- ✅ Captures SSTable paths at snapshot time (true point-in-time view)
- ✅ L0 SSTables checked in reverse order (newest first)
- ✅ 6 comprehensive tests passing

**API**:
```rust
// Lightweight snapshot (SSTable data only)
let snap = db.snapshot();

// Full consistency (forces flush)
let snap = db.snapshot_consistent()?;

// Read from snapshot
snap.get(key)?           // Point lookup
snap.range(start, end)?  // Range scan
snap.sequence_number()   // Sequence number
```

---

## ✅ COMPLETED: Convenience APIs (Nov 16, 2025)

**Implementation**:
- ✅ `db.iter()` - Full table iteration (wrapper around `range(&[], None)`)
- ✅ `db.prefix(prefix)` - Prefix scan with `increment_bytes()` helper
- ✅ Handles 0xFF overflow correctly (e.g., `[0xFF, 0xFF]` → unbounded)
- ✅ 4 comprehensive tests passing

**API**:
```rust
// Full table iteration
for (key, value) in db.iter()? {
    println!("{:?} = {:?}", key, value);
}

// Prefix scan (e.g., all keys starting with "user:")
for (key, value) in db.prefix(b"user:")? {
    println!("{:?} = {:?}", key, value);
}
```

**Deferred to 0.0.2**:
- `ReadOptions`/`WriteOptions` - Per-operation configuration

---

## Current Priority: Stability Testing

---

### Phase 3: Stability Testing (1-2 weeks) - REQUIRED

**Why**: Ensure production reliability before 0.0.1 release

1. **Long-Running Fuzzing** (24h+)
   - All 4 fuzz targets (sstable_parse, wal_parse, vlog_parse, db_operations)
   - Run overnight with crash detection
   - Expand corpus with edge cases

2. **Soak Tests** (72h+)
   - Continuous read/write operations
   - Memory stability (no leaks)
   - Handle recovery (crash injection)
   - File descriptor stability

3. **Chaos Testing**
   - Random process kills during operations
   - Disk full scenarios
   - Network partitions (for future cloud backend)

---

### Phase 4: Documentation & Release (1 week)

**Minimal docs for 0.0.1**:
- API reference (rustdoc)
- Quick start guide
- Configuration options
- 5+ usage examples

**Release checklist**:
- Version tagging (0.0.1)
- CHANGELOG.md
- GitHub release
- crates.io publish (ask first)

---

## Quick Reference: What's Implemented vs Missing

### ✅ IMPLEMENTED (Working)
- Point operations: get(), put(), delete(), batch()
- Range queries: range(start, end) with k-way merge
- **Snapshots**: snapshot(), snapshot_consistent() with get/range
- **Convenience APIs**: iter(), prefix()
- Durability: WAL with configurable sync (SyncAll/SyncData/None)
- Observability: stats(), check_health(), 20+ metrics
- Crash recovery: WAL replay, CRC32 checksums
- Compaction: Leveled + adaptive (Dostoevsky)
- Performance: 2.47x RocksDB writes, 2.07x reads

### ❌ NOT IMPLEMENTED (Priority Order)
1. **Column families** - Multiple namespaces (MEDIUM)
2. **Transactions** - MVCC multi-key atomicity (MEDIUM)
3. **Per-operation options** - ReadOptions/WriteOptions (LOW)
4. **Reverse iteration** - iter_rev() (LOW)
5. **TTL/Expiration** - Automatic key deletion (LOW)
6. **Cloud storage** - S3/GCS backend (LOW for 0.0.1)

---

## CI Status

**Latest fixes** (Nov 16, 2025):
- ✅ Rust edition 2024 → 2021 (2024 doesn't exist)
- ✅ Let-chain syntax converted to nested if-let
- ✅ SIMD feature gates with fallbacks
- ✅ 72 files reformatted with import ordering

**Waiting**: CI run to complete and verify all passes

---

## Next Session Tasks

1. ✅ **Snapshots implemented** - 6 tests passing
2. ✅ **Convenience APIs implemented** - 4 tests passing (iter, prefix)
3. **Long-running fuzzing** - 24h+ stability tests
4. **Documentation** - Update README with snapshot/convenience API examples
5. **0.0.1 release prep** - Version tagging, CHANGELOG

---

## Deferred to 0.0.2+

- VLog garbage collection (not implemented)
- Column families (use key prefixes for now)
- MVCC transactions (batch API is per-operation atomic)
- ReadOptions/WriteOptions (per-operation configuration)
- Cloud storage backend (local only for 0.0.1)

---

**Status**: Snapshots + convenience APIs complete, ready for stability testing
**Timeline**: 2-4 weeks to 0.0.1 (stability testing + docs)
**Quality**: 156 tests passing, 81.54% coverage, ASAN clean
**Performance**: 2.47x RocksDB writes, 2.07x reads 🏆
**Updated**: November 16, 2025

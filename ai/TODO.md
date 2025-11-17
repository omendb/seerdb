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

## Current Priority: Convenience APIs

---

### Phase 2: Convenience APIs (1 week) - MEDIUM PRIORITY

**Why**: Make common patterns easier, match competitor APIs

1. **Full Table Iterator**
   ```rust
   impl DB {
       pub fn iter(&self) -> RangeIterator {
           self.range(&[], None)  // All keys
       }
   }
   ```

2. **Prefix Scan**
   ```rust
   impl DB {
       pub fn prefix(&self, prefix: &[u8]) -> RangeIterator {
           let end = increment_prefix(prefix);
           self.range(prefix, Some(&end))
       }
   }
   ```

3. **Per-Operation Options**
   ```rust
   pub struct ReadOptions {
       verify_checksums: bool,  // Default: true
       fill_cache: bool,        // Default: true
       snapshot: Option<Snapshot>,
   }

   pub struct WriteOptions {
       sync: bool,  // Override WAL sync policy
   }
   ```

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
- Durability: WAL with configurable sync (SyncAll/SyncData/None)
- Observability: stats(), check_health(), 20+ metrics
- Crash recovery: WAL replay, CRC32 checksums
- Compaction: Leveled + adaptive (Dostoevsky)
- Performance: 2.47x RocksDB writes, 2.07x reads

### ❌ NOT IMPLEMENTED (Priority Order)
1. **Snapshots** - Point-in-time consistent views (HIGH)
2. **Convenience APIs** - iter(), prefix(), options (MEDIUM)
3. **Column families** - Multiple namespaces (MEDIUM)
4. **Transactions** - MVCC multi-key atomicity (MEDIUM)
5. **Reverse iteration** - iter_rev() (LOW)
6. **TTL/Expiration** - Automatic key deletion (LOW)
7. **Cloud storage** - S3/GCS backend (LOW for 0.0.1)

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
2. **Convenience APIs** - Add iter(), prefix() helpers
3. **Long-running fuzzing** - 24h+ stability tests
4. **Documentation** - Update README with snapshot examples
5. **0.0.1 release prep** - Version tagging, CHANGELOG

---

## Deferred to 0.0.2+

- VLog garbage collection (not implemented)
- Column families (use key prefixes for now)
- MVCC transactions (batch API is per-operation atomic)
- Cloud storage backend (local only for 0.0.1)

---

**Status**: Feature audit complete, snapshots highest priority
**Timeline**: 4-6 weeks to 0.0.1
**Quality**: 271 tests passing, 81.54% coverage, ASAN clean
**Performance**: 2.47x RocksDB writes, 2.07x reads 🏆
**Updated**: November 16, 2025

# TODO - seerdb

**Last Updated**: November 16, 2025
**Current Sprint**: Feature Completeness (Snapshots + Convenience APIs)
**Previous**: Feature audit revealed range queries WORK, snapshots missing
**Timeline**: 4-6 weeks to 0.0.1

---

## Current Priority: Implement Snapshots

### Phase 1: Snapshots (1-2 weeks) - HIGHEST PRIORITY

**Why**: Without snapshots, no consistent multi-read views. Critical for:
- Range scans during concurrent writes
- Consistent backup
- Multi-key atomic reads
- Long-running queries

**Implementation Plan**:

1. **Snapshot Structure**
   ```rust
   pub struct Snapshot {
       seq_num: u64,                    // Sequence number at snapshot time
       memtables: Vec<Arc<Memtable>>,   // Pinned memtables
       sstables: Vec<Arc<SSTable>>,     // Pinned SSTables
       lsm_tree: Arc<...>,              // Reference to LSM state
   }
   ```

2. **API**
   ```rust
   impl DB {
       pub fn snapshot(&self) -> Snapshot;
   }

   impl Snapshot {
       pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>>;
       pub fn range(&self, start: &[u8], end: Option<&[u8]>) -> RangeIterator;
   }
   ```

3. **Key Requirements**
   - Reference counting for SSTable retention
   - Don't delete SSTables while snapshots hold references
   - Immutable view of LSM tree state at snapshot time
   - Memtable pinning (prevent flush from clearing data)

4. **Tests Needed**
   - Snapshot consistency during writes
   - Snapshot with concurrent compaction
   - Snapshot with concurrent flush
   - Long-lived snapshot (hours)
   - Memory reclamation after snapshot drop

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

1. **Verify CI green** - Check latest run passes
2. **Start snapshot implementation** - Create Snapshot struct
3. **Design retention mechanism** - Reference counting for SSTables
4. **Add snapshot tests** - Consistency during concurrent operations
5. **Update lib.rs exports** - Make Snapshot public

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

# Crash Recovery Test Strategy

**Last Updated**: November 1, 2025
**Status**: ✅ Phase 1 COMPLETE - All 5 tests passing
**Priority**: HIGH-4 (production blocker)

## Phase 1 Completion Summary (November 1, 2025)

✅ **ALL 5 CRASH RECOVERY TESTS PASSING**

**Bugs Fixed**:
1. **CRITICAL**: SSTable corruption not detected on DB::open()
   - Root cause: `LSMTree::new()` created empty LSM tree, never loaded existing SSTables
   - Impact: All persisted data lost on database reopen
   - Fix: Added `load_existing_sstables()` to src/compaction/mod.rs:246-278
   - Calls `SSTable::open()` which validates checksums, detecting corruption

2. **HIGH**: WAL not cleared after flush
   - Impact: WAL grows indefinitely, wastes disk space
   - Fix: Added `WAL::clear()` method to src/wal/mod.rs:140-149
   - Called after successful flush in src/db.rs:369-376

3. **HIGH**: Truncated WAL causes DB::open() to fail
   - Impact: Database won't open after incomplete write (power loss)
   - Fix: Modified `recover()` in src/db.rs:207-242 to handle errors gracefully
   - Now recovers all valid records before truncation/corruption point

**Test Results**: 87/87 tests passing (68 unit + 5 crash recovery + 14 integration)

---

## Current State

**Existing Tests** ✅:
- `test_db_crash_recovery_with_uncommitted_data` - WAL recovery works
- WAL has CRC32 checksums
- SSTables have CRC32 checksums (v1)
- vLog has CRC32 checksums

**Missing Tests** ❌:
- No crash-during-flush tests
- No crash-during-compaction tests
- No SyncPolicy coverage tests
- No corruption recovery tests
- No partial write tests

---

## Test Strategy

### Approach 1: Simulated Crashes (Recommended)

Instead of spawning processes and killing them (complex, flaky), simulate crashes by:
1. **Incomplete operations**: Stop mid-operation, verify recovery
2. **Corrupt data**: Flip bits, verify checksum detection
3. **Partial writes**: Truncate files, verify error handling
4. **State inconsistencies**: Test edge cases in recovery logic

**Pros**:
- Fast, deterministic
- No process spawning complexity
- Works in CI/CD
- Easy to debug

**Cons**:
- Doesn't test actual kill -9 scenarios
- May miss OS-level race conditions

### Approach 2: Process-Based Tests (Future)

Spawn child processes, kill mid-operation:
- Requires separate test binary
- Complex setup (IPC, process management)
- Slow, potentially flaky
- Good for final validation

**Decision**: Start with Approach 1, add Approach 2 in Phase 5 (Real-world validation)

---

## Test Cases

### 1. WAL Recovery Tests (Existing ✅)

**test_db_crash_recovery_with_uncommitted_data**:
- Write data without flush
- Close DB (simulates crash)
- Reopen, verify data recovered from WAL

**Additional Tests Needed**:
- ✅ Recovery with deletes in WAL
- ✅ Recovery with overwrites in WAL
- ❌ Recovery with empty WAL
- ❌ Recovery with partial WAL record (truncated)
- ❌ Recovery with corrupted WAL record (bad CRC)

### 2. Flush Crash Tests (New)

**test_crash_during_flush_incomplete_sstable**:
- Write data, trigger flush
- Simulate crash by not completing flush (leave memtable + partial SSTable)
- Reopen, verify data still in WAL
- Flush completes, verify data persisted

**test_crash_during_flush_wal_deleted_prematurely**:
- Flush SSTable successfully
- Simulate crash before WAL deleted
- Reopen, verify:
  - SSTable exists
  - WAL still exists
  - No duplicate data after recovery

**test_flush_with_different_sync_policies**:
- Test flush with SyncPolicy::SyncAll (safest)
- Test flush with SyncPolicy::SyncData (fast)
- Test flush with SyncPolicy::None (fastest, least durable)
- Verify durability guarantees match expectations

### 3. Compaction Crash Tests (New)

**test_crash_during_compaction_incomplete**:
- Create multiple SSTables at L0
- Start compaction
- Simulate crash mid-compaction (output SSTable incomplete)
- Reopen, verify:
  - Input SSTables still exist
  - Partial output SSTable ignored/deleted
  - Data still readable from input SSTables

**test_crash_during_compaction_after_write**:
- Compaction writes new SSTable successfully
- Simulate crash before updating LSM tree
- Reopen, verify:
  - New SSTable exists but not in LSM tree
  - Input SSTables still in LSM tree
  - Data still readable

**test_compaction_orphaned_sstables**:
- Create SSTables
- Simulate crash leaving orphaned files
- Reopen, verify:
  - Orphaned SSTables detected
  - Can clean up or ignore them
  - Database still functional

### 4. Checksum Recovery Tests (New)

**test_corrupted_sstable_detected**:
- Write SSTable successfully
- Corrupt file (flip bits in data section)
- Reopen, verify:
  - Corruption detected via checksum
  - SSTableError::Corruption returned
  - Database doesn't crash

**test_corrupted_wal_detected**:
- Write WAL records
- Corrupt WAL (flip bits)
- Reopen, verify:
  - Corruption detected via CRC
  - Recovery stops at corrupt record
  - Valid records before corruption are recovered

**test_corrupted_vlog_detected**:
- Write values to vLog
- Corrupt vLog (flip bits)
- Read value, verify:
  - Corruption detected via CRC
  - Error returned to caller
  - Database doesn't crash

### 5. Partial Write Tests (New)

**test_truncated_sstable_recovery**:
- Create SSTable
- Truncate file (simulate incomplete write)
- Reopen, verify:
  - Truncated SSTable detected (footer missing)
  - Error returned
  - Database recovers gracefully

**test_truncated_wal_recovery**:
- Write WAL records
- Truncate WAL (last record incomplete)
- Reopen, verify:
  - Incomplete record ignored
  - Valid records recovered
  - Database functional

### 6. SyncPolicy Tests (New)

**test_sync_all_guarantees**:
- Write with SyncPolicy::SyncAll
- Verify every write is durable
- Simulate crash after each write
- All data recovered

**test_sync_data_guarantees**:
- Write with SyncPolicy::SyncData
- Verify data persisted (metadata may lag)
- Simulate crash
- Data recovered

**test_sync_none_guarantees**:
- Write with SyncPolicy::None
- Verify fast writes (no sync)
- Simulate crash
- Recent data may be lost (expected behavior)
- Database still recovers

---

## Implementation Plan

### Phase 1: Basic Crash Recovery Tests (This Week)

**Priority**: HIGH
**Estimated**: 2-3 days

Tests to implement:
1. ✅ test_crash_during_flush_incomplete_sstable
2. ✅ test_crash_during_compaction_incomplete
3. ✅ test_corrupted_sstable_detected
4. ✅ test_corrupted_wal_detected
5. ✅ test_truncated_wal_recovery

### Phase 2: SyncPolicy Coverage (Next Week)

**Priority**: MEDIUM
**Estimated**: 1-2 days

Tests to implement:
1. ❌ test_sync_all_guarantees
2. ❌ test_sync_data_guarantees
3. ❌ test_sync_none_guarantees

### Phase 3: Edge Cases (Week 3)

**Priority**: LOW
**Estimated**: 1-2 days

Tests to implement:
1. ❌ test_compaction_orphaned_sstables
2. ❌ test_corrupted_vlog_detected
3. ❌ test_truncated_sstable_recovery

### Phase 4: Process-Based Tests (Future - Phase 5)

**Priority**: FUTURE
**Estimated**: 1 week

Real kill -9 tests:
- Spawn child process
- Kill mid-operation
- Verify recovery
- Requires separate test harness

---

## Test Implementation Approach

### File Corruption Simulation

```rust
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

fn corrupt_file(path: &Path, offset: u64, corruption: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(corruption)?;
    file.sync_all()?;
    Ok(())
}
```

### File Truncation Simulation

```rust
fn truncate_file(path: &Path, new_size: u64) -> std::io::Result<()> {
    let file = OpenOptions::new().write(true).open(path)?;
    file.set_len(new_size)?;
    file.sync_all()?;
    Ok(())
}
```

### Incomplete Operation Simulation

```rust
// For flush: Don't delete WAL after flush completes
// For compaction: Don't update LSM tree after compaction completes
// Verify recovery handles these states
```

---

## Success Criteria

**Phase 1 Complete**:
- ✅ 5+ new crash recovery tests
- ✅ All tests pass
- ✅ Corruption detection verified
- ✅ WAL recovery edge cases covered
- ✅ Documentation updated

**Phase 2-3 Complete**:
- ✅ All SyncPolicy modes tested
- ✅ Edge cases covered
- ✅ 10+ total crash recovery tests

**Phase 4 Complete**:
- ✅ Real process-based crash tests
- ✅ Full durability guarantees documented
- ✅ Production confidence achieved

---

## Documentation Requirements

After tests implemented:
1. Document durability guarantees per SyncPolicy
2. Update README with crash recovery behavior
3. Add "Durability" section to ARCHITECTURE.md
4. Update PRODUCTION_ROADMAP.md progress

---

*Last Updated: November 1, 2025*
*Priority: HIGH-4 (production blocker)*
*Timeline: 1 week (Phase 1-2), 2 weeks (Phase 1-3)*

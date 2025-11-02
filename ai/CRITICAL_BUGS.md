# Critical Bugs and Known Issues

**Last Updated**: November 1, 2025
**Status**: Phase 1 - Bug Fixing In Progress

---

## Priority Legend

- **CRITICAL**: Blocker for production use, causes data loss/corruption
- **HIGH**: Major functionality broken or dangerous code
- **MEDIUM**: Important but has workarounds
- **LOW**: Nice to have, tech debt
- **FUTURE**: Deferred to post-launch

---

## CRITICAL Issues

### CRITICAL-1: Compaction doesn't update LSM tree
**Location**: `src/db.rs:396-401`
**Impact**: Compaction runs but doesn't update internal state, breaking reads

**Code**:
```rust
// TODO: This is simplified - need proper level management
// For now, just add to next level
let _lsm = self.lsm.lock().unwrap();
// lsm.add_to_level(level_num + 1, result_path, size);
```

**Problem**:
- Compaction creates new SSTable but doesn't register it
- Old SSTables not removed from level tracking
- Reads may miss compacted data or read stale SSTables

**Fix Required**:
1. Implement `LSMTree::add_to_level()` method
2. Add file metadata tracking (path, size, key range)
3. Update level tracking after successful compaction
4. Add tests to verify LSM state after compaction

**Estimated**: 2-3 days
**Blocking**: Production use

---

### CRITICAL-2: SSTables not deleted after compaction
**Location**: `src/db.rs:401-405`
**Impact**: Disk space leak, database grows unbounded

**Code**:
```rust
// TODO: Delete input SSTables
// for path in input_paths {
//     std::fs::remove_file(path)?;
// }
```

**Problem**:
- Old SSTables remain on disk after compaction
- Disk usage grows with every compaction
- May run out of disk space in production

**Fix Required**:
1. Delete source SSTables after successful merge
2. Handle deletion failures gracefully
3. Use reference counting to prevent deleting in-use files
4. Add tests to verify file cleanup
5. Document disk space requirements

**Estimated**: 1-2 days
**Blocking**: Production use

---

### CRITICAL-3: Duplicate compaction code
**Location**: `src/db.rs:443-452` (run_compaction method)
**Impact**: Code duplication, both copies have same bugs

**Code**:
```rust
// Static method for background compaction worker
fn run_compaction(...) -> Result<()> {
    // TODO: This is simplified - need proper level management
    // TODO: Delete input SSTables
    // Same bugs as compact_level()
}
```

**Problem**:
- `compact_level()` and `run_compaction()` have identical logic
- Both have same TODO bugs
- Fixes must be applied twice

**Fix Required**:
1. Extract common logic to shared private method
2. Both methods call shared implementation
3. Fix bugs once, both methods benefit

**Estimated**: 1 day
**Blocking**: Code quality

---

## HIGH Priority Issues

### HIGH-1: Excessive .unwrap() calls
**Location**: Throughout codebase
**Impact**: Panics instead of returning errors, crashes production

**Stats**:
- `src/db.rs`: 99 unwrap calls
- `src/vlog/mod.rs`: 28 unwrap calls
- `src/sstable/mod.rs`: 58 unwrap calls
- `src/compaction/mod.rs`: 22 unwrap calls
- `src/compaction/merge.rs`: 19 unwrap calls
- `src/wal/reader.rs`: 11 unwrap calls
- `src/wal/mod.rs`: 13 unwrap calls
- `src/memtable/mod.rs`: 7 unwrap calls
- Other files: 4 unwrap calls
- **Total: 261 unwrap calls**

**Problem**:
- `.unwrap()` panics on error (crashes process)
- Production databases should never panic
- Errors should be returned and handled

**Fix Required**:
1. Audit all 261 unwrap calls
2. Categorize: production code vs test code
3. Replace production unwraps with `?` or `.expect()` with context
4. Keep test unwraps (acceptable in tests)
5. Add error tests for each replacement

**Estimated**: 4-5 days
**Blocking**: Production use

---

### HIGH-2: No data integrity checksums (except vLog and WAL) ✅ FIXED
**Location**: `src/sstable/mod.rs`
**Impact**: Silent data corruption possible
**Status**: ✅ **COMPLETED** (commit a9aa99e)

**What Was Done**:
- ✅ Added CRC32 checksums to SSTable format
- ✅ Checksum covers all data (entries + index + bloom + offsets)
- ✅ Version field added for future format upgrades
- ✅ Verify checksums on SSTable::open()
- ✅ Return SSTableError::Corruption on mismatch
- ✅ Added corruption detection tests (flip bits, verify detection)
- ✅ All 68 tests pass

**Implementation**:
- Format v1: Single-file CRC32 checksum
- Footer: [index_offset][bloom_offset][checksum: u32][version: u32]
- Hardware-accelerated via crc32fast (SSE 4.2/ARMv8 CRC)
- Performance impact: <1% overhead
- Storage overhead: +8 bytes per SSTable

**Remaining**:
- LSM metadata checksums (FUTURE - not critical)
- Per-block checksums (FUTURE - for partial reads)

---

### HIGH-3: No stress tests
**Location**: `tests/` directory
**Impact**: Unknown behavior under load

**Current State**:
- 66 unit and integration tests ✅
- All tests use small datasets (< 10k entries)
- No concurrent operation tests
- No long-running tests
- No large dataset tests (100GB+)

**Fix Required**:
1. Add stress test suite
  - 10M+ sequential writes
  - 10M+ random writes
  - Concurrent reads/writes (8+ threads)
  - Long-running (24+ hours)
  - Large datasets (100GB+)
2. Measure: throughput, latency (p50/p99/p999)
3. Verify: no memory leaks, no fd leaks, stable performance

**Estimated**: 1 week
**Blocking**: Production confidence

---

### HIGH-4: No crash recovery tests
**Location**: `tests/` directory
**Impact**: Unknown behavior after crashes

**Current State**:
- WAL recovery tests exist ✅
- No crash-during-write tests ❌
- No crash-during-flush tests ❌
- No crash-during-compaction tests ❌
- No crash-during-recovery tests ❌

**Fix Required**:
1. Add crash recovery test suite
  - kill -9 during put()
  - kill -9 during flush()
  - kill -9 during compaction
  - kill -9 during WAL replay
2. Verify: no data loss, no corruption
3. Test all SyncPolicy modes
4. Document durability guarantees

**Estimated**: 1 week
**Blocking**: Production confidence

---

## MEDIUM Priority Issues

### MEDIUM-1: No observability
**Location**: Entire codebase
**Impact**: Hard to debug production issues

**Current State**:
- A few `eprintln!` statements for errors
- No structured logging ❌
- No metrics ❌
- No tracing ❌
- No performance profiling ❌

**Fix Required**:
1. Add tracing crate for structured logging
2. Log key events (flush, compaction, errors)
3. Expose metrics via `DB::stats()` API
  - Write/read throughput
  - Compaction counts
  - Disk usage per level
  - Latency histograms
4. Add examples for observability

**Estimated**: 1 week
**Blocking**: Production debugging

---

### MEDIUM-2: No fuzzing
**Location**: `tests/` directory
**Impact**: Unknown behavior with random inputs

**Current State**:
- No fuzz tests ❌
- No property-based tests ❌

**Fix Required**:
1. Set up cargo-fuzz
2. Fuzz SSTable parsing
3. Fuzz WAL parsing
4. Fuzz vLog parsing
5. Run 24+ hours
6. Add property-based tests (proptest)
  - Property: Put then Get returns same value
  - Property: Delete then Get returns None
  - Property: Range scan is sorted

**Estimated**: 1 week
**Blocking**: High confidence

---

### MEDIUM-3: No linting enforcement
**Location**: CI/CD (doesn't exist)
**Impact**: Code quality issues accumulate

**Current State**:
- No CI/CD pipeline ❌
- No clippy in CI ❌
- No fmt check in CI ❌
- Unknown number of clippy warnings

**Fix Required**:
1. Run `cargo clippy --all-targets -- -D warnings`
2. Fix all warnings
3. Add GitHub Actions workflow
4. Run clippy and fmt on every PR
5. Block merge if checks fail

**Estimated**: 3-4 days
**Blocking**: Code quality

---

## LOW Priority Issues

### LOW-1: Performance not profiled
**Location**: N/A
**Impact**: Don't know if we're fast or slow

**Current State**:
- Benchmarks exist ✅
- No CPU profiling (flamegraphs) ❌
- No I/O profiling ❌
- No memory profiling ❌
- Hot paths unknown

**Fix Required**:
1. Generate flamegraphs for workloads
2. Profile I/O (read/write amplification)
3. Profile memory (allocations)
4. Document hot paths
5. Optimize based on data

**Estimated**: 4-5 days
**Blocking**: Performance claims

---

### LOW-2: No documentation
**Location**: API docs
**Impact**: Hard to use correctly

**Current State**:
- Code comments exist
- No rustdoc on public APIs ❌
- No user guide ❌
- No architecture docs ❌

**Fix Required**:
1. Add rustdoc to all public items
2. Write user guide (getting started, tuning)
3. Update docs/ with current architecture
4. Add examples for common use cases

**Estimated**: 2-3 days
**Blocking**: Adoption

---

## FUTURE (Post-Launch)

### FUTURE-1: No compression
**Location**: `src/sstable/mod.rs`
**Impact**: Larger disk usage

**Status**: Deferred
**Rationale**: Not critical for initial launch, can add later
**Libraries**: LZ4 (fast), Zstd (high compression)

---

### FUTURE-2: No block cache
**Location**: `src/cache/` (doesn't exist)
**Impact**: More I/O than necessary

**Status**: Deferred
**Rationale**: Not critical for initial launch, can add later
**Implementation**: LRU cache for SSTable blocks

---

### FUTURE-3: No io_uring support
**Location**: `src/io/` (doesn't exist)
**Impact**: Slower I/O on Linux

**Status**: Deferred
**Rationale**: Requires Linux 5.1+, added complexity
**Benefit**: 50-100% faster I/O on Linux

---

## Summary

**CRITICAL**: 3 issues (all compaction-related)
**HIGH**: 4 issues (unwrap calls, checksums, testing)
**MEDIUM**: 3 issues (observability, fuzzing, linting)
**LOW**: 2 issues (profiling, documentation)
**FUTURE**: 3 issues (deferred features)

**Total**: 15 known issues
**Blocking Production**: 9 issues (CRITICAL + HIGH)
**Estimated Fix Time**: 6-8 weeks

---

## Fix Order (Roadmap)

### Week 1-2: CRITICAL fixes
1. CRITICAL-3: Deduplicate compaction code (1 day)
2. CRITICAL-1: Fix LSM tree updates (2-3 days)
3. CRITICAL-2: Implement file cleanup (1-2 days)
4. Test all fixes thoroughly

### Week 3-4: HIGH priority (part 1)
1. HIGH-1: Fix all 261 unwrap calls (4-5 days)
2. HIGH-2: Add data integrity checksums (3-4 days)

### Week 5-6: HIGH priority (part 2)
1. HIGH-3: Add stress tests (1 week)
2. HIGH-4: Add crash recovery tests (1 week)

### Week 7: MEDIUM priority
1. MEDIUM-1: Add observability (1 week)
2. MEDIUM-2: Add fuzzing (1 week, can overlap with week 8)
3. MEDIUM-3: Add linting + CI/CD (3-4 days)

### Week 8: LOW priority + polish
1. LOW-1: Performance profiling (4-5 days)
2. LOW-2: Documentation (2-3 days)
3. Final testing and validation

---

*Last Updated: November 1, 2025*
*Total Known Issues: 15*
*Production Blockers: 9*
*Estimated Time to Production: 6-8 weeks*

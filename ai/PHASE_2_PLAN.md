# Phase 2 Execution Plan - Testing & Validation

**Last Updated**: November 2, 2025
**Status**: Starting Phase 2 (Fuzzing + Leak Detection)
**Completed**: Phase 2.1 (Stress Tests) ✅, Phase 2.2 (Crash Recovery) ✅
**Remaining**: Phase 2.3 (Fuzzing), Phase 2.4 (Leak Detection)

---

## Overview

Phase 2 focuses on comprehensive testing and validation to ensure production readiness. We've already completed stress testing (2.1) and crash recovery testing (2.2) ahead of schedule during Phase 1.

**What's Done**:
- ✅ Phase 2.1: 7 stress tests (100k-1M+ ops, concurrent access, memory leak detection)
- ✅ Phase 2.2: 5 crash recovery tests (corruption detection, graceful recovery)
- ✅ 92/92 tests passing

**What Remains**:
- ⏳ Phase 2.3: Fuzzing & Property-Based Testing (1 week)
- ⏳ Phase 2.4: Leak Detection (2-3 days)

**Timeline**: 1-2 weeks total

---

## Phase 2.3: Fuzzing & Property-Based Testing

**Goal**: Find edge cases and undefined behavior through automated testing

**Priority**: HIGH
**Estimated**: 1 week
**Dependencies**: None (proptest already in dev-dependencies)

### 2.3.1: Set up cargo-fuzz (2-3 days)

**Tasks**:
1. Install cargo-fuzz tooling
   ```bash
   cargo install cargo-fuzz
   ```

2. Initialize fuzz targets
   ```bash
   cargo fuzz init
   ```

3. Create fuzz targets for:
   - **sstable_parse**: Fuzz SSTable::open() with random bytes
   - **wal_parse**: Fuzz WAL parsing with corrupted records
   - **vlog_parse**: Fuzz vLog parsing with random data
   - **db_operations**: Fuzz DB operations (put/get/delete/scan)

4. Run initial fuzzing (24+ hours)
   ```bash
   cargo fuzz run sstable_parse -- -max_total_time=86400
   cargo fuzz run wal_parse -- -max_total_time=86400
   cargo fuzz run vlog_parse -- -max_total_time=86400
   ```

**Expected Outcomes**:
- No panics or crashes during fuzzing
- Graceful error handling for all invalid inputs
- Identified edge cases documented and tested

**Success Criteria**:
- 24+ hours of fuzzing with zero crashes
- All discovered edge cases have regression tests
- Fuzzing integrated into CI (short runs on PR)

### 2.3.2: Property-Based Tests with proptest (2-3 days)

**Tasks**:
1. Implement core database properties:
   ```rust
   // Property: Put then Get returns same value
   proptest! {
       fn put_then_get_returns_same_value(key: Vec<u8>, value: Vec<u8>) {
           let db = DB::open_default(temp_dir())?;
           db.put(&key, &value)?;
           assert_eq!(db.get(&key)?, Some(value));
       }
   }
   ```

2. Key properties to test:
   - **Put-Get invariant**: `put(k,v); get(k) == v`
   - **Delete invariant**: `delete(k); get(k) == None`
   - **Range scan ordering**: Scan results are sorted by key
   - **Compaction preservation**: Data exists after compaction
   - **WAL recovery**: Committed data persists after recovery

3. Test with 10k+ random cases per property

4. Document property test strategy in tests/property_tests.rs

**Expected Outcomes**:
- All core invariants validated
- Edge cases discovered and fixed
- High confidence in correctness

**Success Criteria**:
- 5+ property tests implemented
- 10k+ test cases per property passing
- No property violations found

### 2.3.3: Edge Case Tests (1-2 days)

**Tasks**:
1. Implement edge case test suite:
   - **Empty keys**: `put(b"", value)` - should error or handle gracefully
   - **Empty values**: `put(key, b"")` - should work
   - **Maximum size keys**: 64KB keys (current limit)
   - **Maximum size values**: Test vLog threshold (4KB+)
   - **Binary data**: Non-UTF8 keys/values
   - **Special characters**: Null bytes, control chars
   - **Boundary values**: u32::MAX, i64::MIN, etc.

2. Document edge case behavior in docs/EDGE_CASES.md

3. Add edge case tests to CI

**Expected Outcomes**:
- Clear documentation of edge case handling
- No panics on edge cases
- Graceful errors where appropriate

**Success Criteria**:
- 10+ edge case tests passing
- Edge case behavior documented
- No undefined behavior on edge cases

---

## Phase 2.4: Leak Detection

**Goal**: Ensure no resource leaks (memory, file descriptors, threads)

**Priority**: MEDIUM
**Estimated**: 2-3 days
**Dependencies**: Basic leak detection already in stress tests

### 2.4.1: Memory Leak Detection (1 day)

**Current State**:
- ✅ Basic memory leak detection in stress tests (5x growth threshold)
- ⏳ Need more comprehensive testing

**Tasks**:
1. Add dedicated memory leak test:
   ```rust
   #[test]
   fn test_no_memory_leaks_1m_ops() {
       let db = DB::open_default(temp_dir())?;
       let baseline = get_memory_usage();

       // 1M operations
       for i in 0..1_000_000 {
           db.put(&format!("key{}", i).into_bytes(), b"value")?;
           if i % 100_000 == 0 {
               db.flush()?; // Trigger flush
           }
       }

       let final_memory = get_memory_usage();

       // Should be within 2x of baseline (accounting for caches)
       assert!(final_memory < baseline * 2);
   }
   ```

2. Test leak scenarios:
   - Repeated put/delete cycles
   - Repeated flush operations
   - Repeated compaction operations
   - Long-running database (24+ hours)

3. Use external tools for validation:
   - **valgrind** (Linux): `valgrind --leak-check=full`
   - **heaptrack** (Linux): `heaptrack target/debug/seerdb`
   - **Instruments** (macOS): Memory profiling

**Expected Outcomes**:
- Zero memory leaks detected
- Memory usage bounded by cache sizes
- Stable memory usage over time

**Success Criteria**:
- 1M+ operations with <2x memory growth
- Valgrind/heaptrack show zero leaks
- 24-hour run shows stable memory

### 2.4.2: File Descriptor Leak Detection (1 day)

**Tasks**:
1. Add FD leak test:
   ```rust
   #[test]
   fn test_no_fd_leaks() {
       let baseline_fds = get_open_fds();

       {
           let db = DB::open_default(temp_dir())?;

           // 100k operations + multiple flushes/compactions
           for i in 0..100_000 {
               db.put(&format!("key{}", i).into_bytes(), b"value")?;
               if i % 10_000 == 0 {
                   db.flush()?;
               }
           }

           db.compact_level(0)?;
       } // DB dropped here

       // All files should be closed
       let final_fds = get_open_fds();
       assert_eq!(final_fds, baseline_fds);
   }
   ```

2. Test FD leak scenarios:
   - Database open/close cycles
   - SSTable creation/deletion
   - WAL rotation
   - vLog growth/GC

3. Use lsof to monitor file descriptors:
   ```bash
   # Before test
   lsof -p $(pgrep test) | wc -l

   # After test
   lsof -p $(pgrep test) | wc -l
   ```

**Expected Outcomes**:
- All files closed on DB::drop()
- No file descriptor leaks
- Bounded FD usage

**Success Criteria**:
- FD count returns to baseline after DB drop
- No FD growth over repeated operations
- lsof confirms all files closed

### 2.4.3: Thread Leak Detection (Half day)

**Tasks**:
1. Verify background thread cleanup:
   ```rust
   #[test]
   fn test_background_threads_cleaned_up() {
       let baseline_threads = get_thread_count();

       {
           let db = DB::open(
               DBOptions {
                   enable_background_compaction: true,
                   ..Default::default()
               },
               temp_dir()
           )?;

           // Trigger compaction
           for i in 0..100_000 {
               db.put(&format!("key{}", i).into_bytes(), b"value")?;
           }
           db.flush()?;

           // Background compaction thread should be running
           assert!(get_thread_count() > baseline_threads);
       } // DB dropped

       std::thread::sleep(Duration::from_millis(100));

       // Threads should be joined
       assert_eq!(get_thread_count(), baseline_threads);
   }
   ```

2. Test thread scenarios:
   - Background compaction thread lifecycle
   - Multiple DB instances
   - Graceful shutdown on drop

**Expected Outcomes**:
- Background threads join on DB::drop()
- No dangling threads
- Clean shutdown

**Success Criteria**:
- Thread count returns to baseline after drop
- Background compaction thread joins successfully
- No zombie threads

---

## Implementation Order

### Week 1: Fuzzing & Property Tests

**Days 1-3: Cargo-fuzz setup**
1. Install cargo-fuzz
2. Create fuzz targets (sstable, wal, vlog, db_operations)
3. Run initial 24-hour fuzz campaign
4. Fix any crashes/panics discovered
5. Add regression tests for edge cases

**Days 4-5: Property-based tests**
1. Implement 5 core property tests
2. Run 10k+ cases per property
3. Fix any violations discovered
4. Document property test strategy

**Days 6-7: Edge cases**
1. Implement edge case test suite
2. Document edge case behavior
3. Fix any issues discovered

### Week 2: Leak Detection (Optional - if time permits)

**Day 1: Memory leaks**
1. Implement dedicated memory leak tests
2. Run valgrind/heaptrack
3. Fix any leaks discovered
4. Run 24-hour stability test

**Day 2: FD leaks**
1. Implement FD leak tests
2. Monitor with lsof
3. Fix any leaks discovered

**Day 3: Thread leaks + Buffer**
1. Implement thread leak tests
2. Verify background thread cleanup
3. Buffer day for any issues

---

## Success Metrics

### Phase 2.3 Success
- ✅ 24+ hours fuzzing with zero crashes
- ✅ 5+ property tests with 10k+ cases each
- ✅ 10+ edge case tests passing
- ✅ All discovered issues fixed and documented

### Phase 2.4 Success
- ✅ Zero memory leaks (valgrind/heaptrack clean)
- ✅ Zero FD leaks (lsof confirms)
- ✅ Background threads cleaned up properly
- ✅ 24-hour run shows stable resources

### Overall Phase 2 Success
- ✅ 100+ total tests passing (92 current + new tests)
- ✅ High confidence in correctness (property tests)
- ✅ High confidence in robustness (fuzzing)
- ✅ High confidence in stability (leak detection)
- ✅ Ready for Phase 3 (Observability)

---

## Risk Mitigation

**Risk**: Fuzzing discovers critical bugs
- **Mitigation**: Fix immediately, add regression tests
- **Fallback**: Document known issues, plan fixes for Phase 3

**Risk**: Property tests reveal design flaws
- **Mitigation**: Fix design issues, may require refactoring
- **Fallback**: Document limitations, plan redesign

**Risk**: Memory leaks difficult to fix
- **Mitigation**: Profile with heaptrack, identify root cause
- **Fallback**: Document leak, add workaround (periodic restart)

**Risk**: Timeline slips (fuzzing takes longer than 24h)
- **Mitigation**: Run fuzzing in background, continue with other tasks
- **Fallback**: Reduce fuzzing time to 12 hours, iterate

---

## Tools Required

**Fuzzing**:
- cargo-fuzz (libfuzzer wrapper)
- AFL++ (optional, for comparison)

**Property Testing**:
- proptest (already in Cargo.toml)

**Leak Detection**:
- valgrind (Linux)
- heaptrack (Linux)
- Instruments (macOS)
- lsof (FD monitoring)
- sysinfo (already in Cargo.toml)

**CI Integration**:
- GitHub Actions for fuzzing (short runs)
- Dedicated fuzzing server (24+ hour runs)

---

## Next Steps

**Immediate** (Today):
1. ✅ Update PLAN.md
2. ✅ Create this execution plan
3. ⏳ Install cargo-fuzz
4. ⏳ Create first fuzz target (sstable_parse)
5. ⏳ Start 24-hour fuzz run

**This Week**:
1. Complete all fuzz targets
2. Implement property tests
3. Add edge case tests
4. Fix any issues discovered

**Next Week** (if needed):
1. Comprehensive leak detection
2. 24-hour stability test
3. Final validation
4. Phase 2 completion report

---

*Last Updated: November 2, 2025*
*Status: Phase 2 starting (2.1 and 2.2 complete)*
*Owner: seerdb team*
*Estimated Completion: November 16, 2025*

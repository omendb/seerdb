# seerdb Testing Strategy - Path to 80%+ Coverage

**Date**: November 10, 2025
**Current Status**: 287 tests (279 passing, 8 ignored), ~15% coverage estimate
**Goal**: 80%+ code coverage for 0.0.1 release
**Timeline**: Week 5-6 of 8-week roadmap

---

## Current Test Inventory

### Test Files (19 total, 8,301 LOC)

| Test File | Tests | Focus Area | Status |
|-----------|-------|------------|--------|
| batch_atomicity_tests.rs | 4 | Batch API atomic semantics | ✅ Complete |
| compaction_correctness_tests.rs | 8 | Compaction data integrity | ✅ Complete |
| concurrent_edge_case_tests.rs | 14 | Concurrent operations | ✅ Good |
| config_edge_case_tests.rs | ~8 | Configuration edge cases | ✅ Good |
| corruption_detection_tests.rs | ? | Checksum validation | ⚠️ Unknown |
| crash_recovery_tests.rs | ? | WAL recovery | ⚠️ Unknown |
| db_integration_test.rs | ? | End-to-end DB lifecycle | ⚠️ Unknown |
| edge_case_tests.rs | ? | General edge cases | ⚠️ Unknown |
| integration_test.rs | ? | WAL + Memtable + SSTable | ⚠️ Unknown |
| io_failure_tests.rs | ? | I/O error handling | ⚠️ Unknown |
| iterator_tests.rs | ? | Range scans, iteration | ⚠️ Unknown |
| leak_detection_tests.rs | 8 | Memory/FD leaks | ✅ Good |
| minimal_hang_repro.rs | 1 | Hang debugging | ✅ Complete |
| production_hardening_tests.rs | 15 | Memory budget, disk space, panics | ✅ Excellent |
| property_tests.rs | 8 | Property-based testing | ✅ Good |
| snapshot_consistency_tests.rs | 9 | Read consistency | ✅ Good |
| soak_test.rs | 5 (ignored) | Long-running stability | ⏸️ Manual |
| stress_test.rs | 7 (2 ignored) | High-load behavior | ✅ Good |
| **TOTAL** | **287** | | **279 passing** |

---

## Critical Coverage Gaps (High Priority)

### 1. ALEX Learned Index (src/alex/) - ~20% coverage

**Missing Tests**:
- Node split logic (when node exceeds capacity)
- Node merge logic (when node underflows)
- Multi-level tree traversal (root → inner → leaf)
- Bulk loading (initial index construction)
- Error prediction bounds (validate O(log error) guarantee)
- Concurrent modifications (thread safety)

**Action**: Add tests/alex_learned_index_tests.rs (~300 LOC, 15 tests)

### 2. VLog (src/vlog/) - ~30% coverage

**Missing Tests**:
- VLog corruption detection (checksum validation)
- VLog truncation handling (partial writes)
- VLog header validation (magic number, version)
- VLog rotation (when file exceeds size limit)
- VLog concurrent reads (multiple readers)

**Action**: Add tests/vlog_tests.rs (~400 LOC, 20 tests)

### 3. SSTable Block Parsing (src/sstable/block.rs) - ~40% coverage

**Missing Tests**:
- Prefix compression edge cases (empty prefix, full key prefix)
- Varint decoding errors (truncated, invalid)
- Block corruption (CRC mismatch, invalid format)
- Block size limits (minimum, maximum)
- Entry count mismatch (header vs actual)

**Action**: Extend tests/corruption_detection_tests.rs (~200 LOC, 10 tests)

### 4. Compaction Leveled Strategy (src/compaction/mod.rs) - ~40% coverage

**Missing Tests**:
- Multi-level cascading compaction (L0→L1→L2→...)
- Size ratio enforcement (10x between levels)
- Overlapping key ranges (L0 → L1 merges)
- Compaction throttling (when too many L0 files)
- Compaction cancellation (on DB close)

**Action**: Extend tests/compaction_correctness_tests.rs (~300 LOC, 15 tests)

### 5. WAL Recovery Edge Cases (src/wal/) - ~60% coverage

**Missing Tests**:
- Partial record writes (truncated at end)
- WAL header corruption (magic number mismatch)
- WAL multiple file rotation (when WAL spans multiple files)
- WAL recovery with batch records (batch atomicity on recovery)
- WAL recovery performance (large WAL replay latency)

**Action**: Extend tests/crash_recovery_tests.rs (~200 LOC, 10 tests)

---

## Testing Implementation Plan

### Phase 1: Critical Gaps (Week 5, Days 1-3)

**Goal**: Add 100-120 new tests, target +20% coverage

1. Day 1: ALEX Tests (~300 LOC, 15 tests) - Target: +5% coverage
2. Day 2: VLog Tests (~400 LOC, 20 tests) - Target: +5% coverage
3. Day 3: Compaction Tests (~300 LOC, 15 tests) - Target: +5% coverage

**Milestone**: ~70% coverage after Phase 1

### Phase 2: Medium Priority (Week 5, Days 4-5)

**Goal**: Add 50-60 new tests, target +10% coverage

4. Day 4: SSTable + WAL Tests (~400 LOC, 20 tests) - Target: +5% coverage
5. Day 5: Iterator + Memtable Tests (~350 LOC, 18 tests) - Target: +5% coverage

**Milestone**: ~80% coverage after Phase 2

### Phase 3: Polish (Week 6, Day 1)

**Goal**: Add 20-30 tests to reach 85%+

6. Day 6: Remaining Gaps (~250 LOC, 13 tests) - Target: +5% coverage

**Final Milestone**: 85%+ coverage

### Phase 4: Sanitizer Runs (Week 6, Days 2-3)

7. Day 7: Address Sanitizer (ASAN) - Detect: Use-after-free, buffer overflows
8. Day 8: Thread Sanitizer (TSAN) - Detect: Data races, deadlocks

---

## Success Criteria

### Coverage Targets

- [ ] **Overall**: 80%+ line coverage
- [ ] **Critical modules**: 90%+ coverage (WAL, Memtable, SSTable, Compaction)
- [ ] **Complex modules**: 70%+ coverage (ALEX, VLog, Bloom)

### Sanitizer Criteria

- [ ] ASAN: No memory errors detected
- [ ] TSAN: No data races detected
- [ ] All tests pass under sanitizers

---

**Status**: Phase 0 - Coverage analysis running
**Next**: Phase 1 Day 1 - Implement ALEX tests after coverage results

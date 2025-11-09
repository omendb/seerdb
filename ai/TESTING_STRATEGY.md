# Comprehensive Testing Strategy

**Goal**: Achieve 80%+ test coverage for 0.0.1 release
**Current**: ~250+ tests (128 integration + ~120 unit tests)
**Estimated Coverage**: ~15-20% (to be confirmed by tarpaulin)

## Test Inventory (Current)

### Integration Tests (tests/)
- **Crash Recovery**: 15 tests (crash_recovery_test.rs + crash_recovery_tests.rs)
- **Concurrency**: 8 tests (concurrent_edge_case_tests.rs)
- **Edge Cases**: 18 tests (edge_case_tests.rs)
- **Config Edge Cases**: 15 tests (config_edge_case_tests.rs)
- **I/O Failures**: 8 tests (io_failure_tests.rs)
- **Corruption Detection**: 7 tests (corruption_detection_tests.rs)
- **Iterator Tests**: 14 tests (iterator_tests.rs)
- **Snapshot Consistency**: 9 tests (snapshot_consistency_tests.rs)
- **Leak Detection**: 8 tests (leak_detection_tests.rs)
- **Stress**: 7 tests (stress_test.rs)
- **Soak**: 5 tests (soak_test.rs)
- **Integration**: 14 tests (db_integration_test.rs + integration_test.rs)

### Unit Tests (src/)
- **DB Core**: 20 tests (src/db.rs)
- **SIMD**: 15 tests (src/simd.rs)
- **ALEX Index**: 36 tests (alex/ modules)
- **Memtable**: 9 tests (src/memtable/mod.rs)
- **Compaction**: 14 tests (compaction/ modules)
- **WAL**: 8 tests (wal/ modules)
- **VLog**: 6 tests (src/vlog/mod.rs)
- **Bloom Filters**: 9 tests (bloom/ modules)
- **Other**: ~23 tests (various modules)

**Total**: ~250+ tests

---

## Critical Gaps (Must Fix for 80%+ Coverage)

### 1. Production Hardening (NEW - Nov 9, 2025)
**Current**: 0 tests 🚨
**Target**: 15+ tests
**Priority**: CRITICAL

Tests needed:
- Memory budget enforcement (3 tests)
  - Test write blocking at 95% memory
  - Test early flush at 80% memory
  - Test memory estimation accuracy
- Disk space checks (3 tests)
  - Test write rejection when disk full
  - Test disk space validation on startup
  - Test configurable disk space thresholds
- Background thread panic detection (5 tests)
  - Test WAL writer panic detection
  - Test flush thread panic detection
  - Test compaction thread panic detection
  - Test health status propagation
  - Test graceful degradation on panic
- File descriptor limits (2 tests)
  - Test FD usage estimation
  - Test behavior near FD limits
- SSTable fsync validation (2 tests)
  - Test fsync on SSTable creation
  - Test data durability after fsync

### 2. Batch API Atomicity
**Current**: 3 tests (src/batch.rs)
**Target**: 15+ tests
**Priority**: CRITICAL

Tests needed:
- Single WAL record for batches (2 tests)
  - Test batch written as single record
  - Test WAL replay preserves batch atomicity
- Partial batch failure (3 tests)
  - Test batch rollback on error
  - Test no partial writes visible
  - Test recovery after batch failure
- Concurrent batches (3 tests)
  - Test multiple batches don't interfere
  - Test batch ordering preserved
  - Test concurrent batch commit
- Large batches (2 tests)
  - Test 1000+ operations in batch
  - Test batch memory limits
- Mixed operations (3 tests)
  - Test put+delete in same batch
  - Test overwrite in batch
  - Test delete+put same key in batch
- Batch sync policies (2 tests)
  - Test SyncData on batch
  - Test SyncAll on batch

### 3. Compaction Correctness
**Current**: 14 tests
**Target**: 30+ tests
**Priority**: HIGH

Tests needed:
- Live key preservation (5 tests)
  - Test compaction doesn't delete live keys
  - Test sequence number coordination
  - Test concurrent writes during compaction
  - Test key resurrection prevention
  - Test tombstone handling
- Multi-level compaction (3 tests)
  - Test L0→L1→L2 cascade
  - Test level ratio maintenance
  - Test compaction priority
- Range overlaps (3 tests)
  - Test overlapping SSTable merge
  - Test range boundary handling
  - Test key ordering preservation
- Compaction interruption (3 tests)
  - Test graceful shutdown during compaction
  - Test crash during compaction recovery
  - Test compaction resume

### 4. VLog (Value Log)
**Current**: 6 tests
**Target**: 20+ tests
**Priority**: HIGH

Tests needed:
- Large value handling (4 tests)
  - Test 4KB threshold
  - Test 1MB values
  - Test mixed small/large values
  - Test VLog space reclamation
- VLog recovery (3 tests)
  - Test VLog replay on open
  - Test corrupt VLog detection
  - Test VLog truncation recovery
- VLog GC (5 tests) - DEFERRED TO 0.0.2+
  - Test stale value identification
  - Test GC doesn't delete live values
  - Test GC concurrent with reads
  - Test GC space reclamation
  - Test GC scheduling
- VLog corruption (3 tests)
  - Test CRC validation
  - Test partial write detection
  - Test magic number validation

### 5. Cache (Block Cache)
**Current**: ~5 tests
**Target**: 15+ tests
**Priority**: HIGH

Tests needed:
- LRU eviction (4 tests)
  - Test LRU eviction policy
  - Test cache size limits (10K blocks)
  - Test memory pressure handling
  - Test eviction preserves hot blocks
- Cache hit/miss (3 tests)
  - Test cache hit rates
  - Test cache warming
  - Test cache invalidation
- Concurrent cache access (3 tests)
  - Test multi-threaded reads
  - Test cache contention
  - Test cache consistency

### 6. Concurrent Stress Tests
**Current**: 8 tests
**Target**: 25+ tests
**Priority**: HIGH

Tests needed:
- Multi-threaded writes (3 tests)
  - Test 10+ threads concurrent writes
  - Test write ordering
  - Test no lost writes
- Multi-threaded reads (2 tests)
  - Test concurrent read consistency
  - Test read isolation
- Mixed workload stress (5 tests)
  - Test reads+writes concurrently
  - Test scans+writes concurrently
  - Test deletes+reads concurrently
  - Test batches+single ops concurrently
  - Test all operations mixed
- Resource exhaustion (3 tests)
  - Test high memory pressure
  - Test high disk I/O
  - Test high compaction load

### 7. Failure Injection
**Current**: 8 tests (io_failure_tests.rs)
**Target**: 25+ tests
**Priority**: MEDIUM

Tests needed:
- Disk errors (5 tests)
  - Test disk full during write
  - Test disk full during compaction
  - Test disk read errors
  - Test disk write errors
  - Test fsync failure
- OOM simulation (3 tests)
  - Test allocation failure handling
  - Test large value OOM
  - Test memtable flush under OOM
- I/O errors (5 tests)
  - Test WAL write failure
  - Test SSTable read failure
  - Test VLog write failure
  - Test metadata corruption
  - Test file deletion failure
- Network/filesystem delays (2 tests)
  - Test slow fsync
  - Test slow reads

---

## Testing Roadmap (Priority Order)

### Week 1: Production Hardening Tests (15 tests)
**Days 1-2**: Memory budget + disk space tests (6 tests)
**Days 3-4**: Background panic tests (5 tests)
**Days 5**: FD limits + fsync tests (4 tests)

**Deliverable**: Production hardening fully tested

### Week 2: Batch API + Compaction (31 tests)
**Days 1-3**: Batch atomicity tests (15 tests)
**Days 4-5**: Compaction correctness tests (16 tests)

**Deliverable**: Critical data safety tests complete

### Week 3: VLog + Cache + Concurrency (40 tests)
**Days 1-2**: VLog tests (15 tests)
**Days 3**: Cache tests (10 tests)
**Days 4-5**: Concurrent stress tests (15 tests)

**Deliverable**: Component tests complete

### Week 4: Failure Injection + Coverage (25 tests)
**Days 1-2**: Disk errors + OOM tests (10 tests)
**Days 3**: I/O errors + delays (7 tests)
**Days 4-5**: Fill coverage gaps, achieve 80%+ (8+ tests)

**Deliverable**: 80%+ test coverage achieved

---

## Test Coverage Measurement

### Tools
- **cargo-tarpaulin**: Line coverage (target: 80%+)
- **cargo-llvm-cov**: Alternative coverage tool
- **Coverage reports**: HTML reports for visualization

### Coverage Targets by Module
- **db.rs**: 80%+ (critical path)
- **batch.rs**: 90%+ (atomicity critical)
- **compaction/**: 75%+ (complex logic)
- **vlog/**: 80%+ (data safety)
- **wal/**: 85%+ (durability critical)
- **memtable/**: 75%+ (concurrent access)
- **sstable/**: 80%+ (data format)
- **cache**: 70%+ (performance optimization)
- **alex/**: 65%+ (learned index, research code)
- **bloom/**: 70%+ (filter accuracy)

### Continuous Monitoring
```bash
# Run coverage after every test addition
cargo tarpaulin --lib --ignore-tests --out Html --output-dir coverage/

# Fail if coverage drops below 80%
cargo tarpaulin --lib --ignore-tests --fail-under 80
```

---

## Test Quality Standards

### All Tests Must:
1. Be deterministic (no flaky tests)
2. Run in <5 seconds (except stress/soak tests)
3. Clean up resources (tempdir, files, threads)
4. Test one thing (focused assertions)
5. Have clear failure messages
6. Use property-based testing where applicable

### Integration Tests Should:
1. Test end-to-end workflows
2. Test crash recovery scenarios
3. Test concurrent operations
4. Test production configurations
5. Validate durability guarantees

### Unit Tests Should:
1. Test individual functions
2. Test edge cases thoroughly
3. Test error handling
4. Test boundary conditions
5. Be fast (<100ms each)

---

## Success Criteria

### For 0.0.1 Release:
- [ ] **80%+ line coverage** (verified by tarpaulin)
- [ ] **All 7 critical bugs tested** (batch, compaction, cache, etc.)
- [ ] **100% production hardening tested** (memory, disk, panics)
- [ ] **Zero test failures** (all tests pass consistently)
- [ ] **No flaky tests** (all tests deterministic)
- [ ] **Sanitizers clean** (ASAN, MSAN, TSAN pass)
- [ ] **Stress tests pass** (10+ hour soak test)

### Metrics to Track:
- Test count: ~250 → ~400+ tests
- Coverage: ~15% → 80%+
- Test execution time: <2 minutes total
- Zero known data safety issues

---

**Last Updated**: November 9, 2025
**Status**: Planning phase
**Next**: Measure baseline coverage with tarpaulin

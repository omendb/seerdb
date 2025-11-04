# Practical Soak Tests - Quick Reference

**Status**: Ready to run
**Location**: `tests/soak_test.rs`

---

## Recommended Tests

### 1. 2-Hour Continuous Operation (test_2hour_soak)

**Purpose**: Validate memory stability and long-term operation
**Duration**: 2 hours
**Operations**: ~7 million mixed read/write (70% read, 30% write)
**Monitoring**: Reports every minute

**Run**:
```bash
cargo test --release --test soak_test test_2hour_soak -- --ignored --nocapture
```

**What it validates**:
- No memory leaks (<3x initial memory)
- Stable throughput (~1000 ops/sec)
- Background compaction works correctly
- No performance degradation

**Success criteria**:
- ✅ Completes 2 hours without crash
- ✅ Memory stays <3x initial
- ✅ Throughput stays >500 ops/sec

---

### 2. 10GB Dataset (test_10gb_dataset)

**Purpose**: Validate multi-level LSM behavior and large-scale data
**Duration**: 10-30 minutes (depends on disk speed)
**Operations**: Write 10.5M keys (10GB), 100k random reads
**Monitoring**: Reports every 5% or 30 seconds

**Run**:
```bash
cargo test --release --test soak_test test_10gb_dataset -- --ignored --nocapture
```

**What it validates**:
- LSM compaction across multiple levels
- Memory stays bounded during large writes
- Read performance at scale
- No data corruption

**Success criteria**:
- ✅ Successfully writes 10GB
- ✅ Memory stays <4x initial during writes
- ✅ All 100k random reads succeed
- ✅ Final memory <3x initial

---

## When to Run

**Before Production**:
- Run both tests on target hardware
- Document memory usage patterns
- Establish performance baselines

**Before Major Releases**:
- Run 2-hour test overnight
- Run 10GB test as part of CI/CD (if resources allow)

**For Quick Validation**:
- Run 10GB test (faster, still validates LSM behavior)

---

## Extreme Tests (Optional)

For additional validation, see `ai/SOAK_TESTING.md` for:
- `test_24hour_soak_extreme`: 24 hours continuous
- `test_100gb_dataset_extreme`: 100GB dataset

These are **not required** for production readiness but provide extra confidence.

---

## Next Steps After Passing

Once both practical tests pass:

1. **Document results** in `ai/SOAK_RESULTS.md`:
   - Memory usage patterns
   - Throughput over time
   - Performance baselines

2. **Move to Phase 5.2** (Real Workload Validation):
   - Integrate with omen
   - Test actual production workload

3. **Production deployment**: Engine is validated for production use

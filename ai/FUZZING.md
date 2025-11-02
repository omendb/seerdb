# Fuzzing Setup and Results

**Last Updated**: November 2, 2025
**Status**: Fuzzing infrastructure complete, initial tests passing
**Fuzz Targets**: 4 (sstable_parse, wal_parse, vlog_parse, db_operations)

---

## Summary

Fuzzing infrastructure is now fully set up using cargo-fuzz (libfuzzer). All 4 fuzz targets compile and run successfully with no crashes in initial testing.

**Quick Stats** (initial 5-second test of sstable_parse):
- **Executions**: 35,952 in 6 seconds (~5,992 exec/s)
- **Coverage**: 197 code features covered
- **Corpus**: 18 interesting test cases discovered
- **Crashes**: 0 ✅

---

## Fuzz Targets

### 1. sstable_parse
**Purpose**: Fuzz SSTable::open() with random byte sequences
**What it tests**:
- SSTable header parsing
- Index parsing
- Bloom filter parsing
- Checksum validation
- Graceful error handling on corrupted data

**Location**: `fuzz/fuzz_targets/sstable_parse.rs`

**How to run**:
```bash
# Short test (5 seconds)
cargo +nightly fuzz run sstable_parse -- -max_total_time=5

# 1 hour run
cargo +nightly fuzz run sstable_parse -- -max_total_time=3600

# Until crash (manual stop with Ctrl+C)
cargo +nightly fuzz run sstable_parse
```

**Initial Results** (5-second test):
- ✅ 35,952 executions, 0 crashes
- Coverage: 197 features
- Corpus: 18 test cases

### 2. wal_parse
**Purpose**: Fuzz WAL parsing with corrupted records
**What it tests**:
- WAL header parsing
- Record parsing (Put/Delete)
- CRC validation
- Graceful handling of truncated records
- Error recovery

**Location**: `fuzz/fuzz_targets/wal_parse.rs`

**How to run**:
```bash
cargo +nightly fuzz run wal_parse -- -max_total_time=3600
```

### 3. vlog_parse
**Purpose**: Fuzz vLog (value log) parsing and reading
**What it tests**:
- vLog header parsing
- Value reading at random offsets
- Checksum validation
- Graceful handling of invalid offsets
- File corruption detection

**Location**: `fuzz/fuzz_targets/vlog_parse.rs`

**How to run**:
```bash
cargo +nightly fuzz run vlog_parse -- -max_total_time=3600
```

### 4. db_operations
**Purpose**: Fuzz high-level DB operations (put/get/delete/scan/flush)
**What it tests**:
- API robustness with random inputs
- Empty keys/values
- Large keys/values (up to limits)
- Binary data (non-UTF8)
- Operation sequencing
- Flush behavior

**Location**: `fuzz/fuzz_targets/db_operations.rs`

**How to run**:
```bash
cargo +nightly fuzz run db_operations -- -max_total_time=3600
```

**Limits** (to avoid OOM):
- Max key size: 64KB
- Max value size: 1MB

---

## Installation

### 1. Install cargo-fuzz
```bash
cargo install cargo-fuzz
```

### 2. Install nightly Rust
```bash
rustup install nightly
```

No need to change default toolchain - cargo-fuzz uses nightly automatically.

---

## Running Fuzzing

### Quick Test (5 seconds per target)
```bash
# Test all targets quickly
for target in sstable_parse wal_parse vlog_parse db_operations; do
    echo "Testing $target..."
    cargo +nightly fuzz run $target -- -max_total_time=5
done
```

### Short Run (1 hour per target)
```bash
# Run each target for 1 hour
for target in sstable_parse wal_parse vlog_parse db_operations; do
    echo "Fuzzing $target for 1 hour..."
    cargo +nightly fuzz run $target -- -max_total_time=3600
done
```

### Long Run (24+ hours)
```bash
# Run individual target for 24 hours
cargo +nightly fuzz run sstable_parse -- -max_total_time=86400
```

### Background Run
```bash
# Run in background with nohup
nohup cargo +nightly fuzz run sstable_parse -- -max_total_time=86400 &

# Monitor progress
tail -f nohup.out
```

---

## Understanding Output

### Coverage Stats
```
cov: 197 ft: 223 corp: 18/974b
```
- **cov: 197** - Number of code features covered
- **ft: 223** - Number of feedback events
- **corp: 18/974b** - 18 test cases in corpus, 974 bytes total

### Execution Stats
```
exec/s: 5992 rss: 80Mb
```
- **exec/s: 5992** - Executions per second (throughput)
- **rss: 80Mb** - Resident set size (memory usage)

### Corpus Growth
```
#12184 REDUCE cov: 196 ft: 220 corp: 15/363b
```
- Test case simplified (REDUCE) or new case found (NEW)
- Coverage and corpus size tracked

### Recommended Dictionary
Fuzzer learns byte patterns that trigger interesting behavior:
```
"\001\000\000\000" # Uses: 739
"\010\000\000\000\000\000\000\000" # Uses: 298
```
These are used to guide future mutations.

---

## Crashes and Artifacts

### Finding Crashes
If fuzzing finds a crash:
```
==12345==ERROR: AddressSanitizer: heap-buffer-overflow
```

Artifact saved to:
```
fuzz/artifacts/<target>/<crash-hash>
```

### Reproducing Crashes
```bash
# Reproduce a specific crash
cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<crash-hash>
```

### Minimizing Crashes
```bash
# Minimize crash input
cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/<crash-hash>
```

### Analyzing Coverage
```bash
# Generate coverage report
cargo +nightly fuzz coverage <target>
```

---

## Current Status

### ✅ Completed
- Fuzzing infrastructure set up
- 4 fuzz targets implemented
- All targets compile successfully
- Initial test run: 0 crashes in 5 seconds

### ⏳ In Progress
- Running longer fuzz campaigns (1-2 hours per target)
- Monitoring for crashes
- Building corpus of interesting test cases

### 📋 Remaining
- 24+ hour fuzz runs
- Fix any discovered crashes
- Add regression tests for crashes
- Integrate fuzzing into CI (short runs)

---

## Integration with CI

### GitHub Actions Workflow (Future)
```yaml
name: Fuzzing

on: [pull_request]

jobs:
  fuzz:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: dtolnay/rust-toolchain@nightly
      - run: cargo install cargo-fuzz
      - run: |
          # Run each fuzz target for 60 seconds
          for target in sstable_parse wal_parse vlog_parse db_operations; do
            cargo fuzz run $target -- -max_total_time=60
          done
```

**Rationale**: Short runs on CI catch obvious issues, long runs on dedicated server.

---

## Best Practices

### 1. Run fuzzing after every major change
- Especially changes to parsing code
- Especially changes to error handling

### 2. Save interesting crashes
- Add crash input as regression test
- Document root cause in git commit

### 3. Monitor corpus growth
- Large corpus = good coverage
- Corpus should stabilize after several hours

### 4. Use dictionaries
- Fuzzer learns effective byte patterns
- Can be manually seeded for known formats

### 5. Parallelize fuzzing
- Run multiple fuzz targets simultaneously
- Use -jobs=N for parallel execution within a target

---

## Fuzzing Goals (Phase 2.3)

### Success Criteria
- ✅ 24+ hours of fuzzing per target
- ✅ Zero crashes discovered (or all fixed)
- ✅ Corpus size stable (no new interesting cases)
- ✅ All discovered edge cases have regression tests

### Timeline
- **Week 1**: Fuzz target setup (DONE)
- **Week 1**: Initial fuzz runs (1-2 hours per target)
- **Week 1-2**: Long fuzz runs (24+ hours per target)
- **Week 2**: Fix any crashes, add regression tests

---

## Resources

- **cargo-fuzz book**: https://rust-fuzz.github.io/book/cargo-fuzz.html
- **libfuzzer docs**: https://llvm.org/docs/LibFuzzer.html
- **Fuzzing strategies**: https://rust-fuzz.github.io/book/
- **AFL++** (alternative): https://aflplus.plus/

---

*Last Updated: November 2, 2025*
*Status: Fuzzing infrastructure complete, initial tests passing*
*Next: Run 1-2 hour fuzz campaigns on all targets*

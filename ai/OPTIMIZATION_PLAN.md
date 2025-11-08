# Micro-Optimization Plan (Closing fjall Gap)

**Goal**: Close 24% gap vs fjall on mixed workload (473K → 600K+ ops/sec)
**Approach**: Test fjall's proven optimizations one-by-one
**Timeline**: 3-5 days for quick wins

## Optimizations to Test

### 1. varint-rs (Priority 1 - Already Added) ⏱️ 2-3 hours

**Current**: Custom varint implementation in `src/sstable/block.rs`
**New**: Use `varint-rs` crate (same as fjall uses)

**Expected impact**: +1-3% (varint encoding is on hot path)

**Implementation**:
```rust
// Replace custom varint_encode/varint_decode with:
use varint_rs::{VarintWriter, VarintReader};

// In block encoding:
let mut writer = Vec::new();
writer.write_u32_varint(value)?;

// In block decoding:
let value = reader.read_u32_varint()?;
```

**Files to change**:
- `src/sstable/block.rs` - Replace varint functions
- Search for `varint_encode`, `varint_decode` usage

**Validation**:
- Run all tests (should pass with correct implementation)
- Benchmark: Should see 1-3% improvement

### 2. quick_cache for Block Cache (Priority 2 - Already Added) ⏱️ 3-4 hours

**Current**: `HashMap<PathBuf, Arc<Mutex<SSTable>>>` in `src/db.rs`
**New**: `quick_cache::sync::Cache` (optimized, thread-safe)

**Expected impact**: +3-5% (cache is accessed on every read)

**Implementation**:
```rust
use quick_cache::sync::Cache;

// Replace sstable_cache HashMap with:
sstable_cache: Cache<PathBuf, Arc<Mutex<SSTable>>>,

// Initialization:
let cache = Cache::new(1000); // 1000 entries

// Usage:
cache.get_or_insert_with(path, || open_sstable(path));
```

**Files to change**:
- `src/db.rs` - Replace HashMap with Cache
- Update all cache.get/insert calls

**Validation**:
- Run all tests
- Benchmark: Should see 3-5% improvement

### 3. More Aggressive Compaction (Priority 3) ⏱️ 2-3 hours

**Current**: Background compaction may be too conservative
**New**: Trigger compaction earlier, merge more aggressively

**Expected impact**: +5-10% (reduces read amplification)

**Investigation needed**:
```rust
// Check current compaction trigger in src/compaction/mod.rs
// Look for level size ratios and trigger points
```

**Potential changes**:
1. Lower size_ratio (currently 10, try 5-7)
2. Trigger compaction at lower thresholds
3. Increase compaction parallelism

**Files to investigate**:
- `src/compaction/mod.rs`
- `src/db.rs` - compaction trigger logic

**Validation**:
- Monitor SSTable count during benchmark
- Should see fewer SSTables = better read performance

### 4. Inline Small Functions (Priority 4) ⏱️ 1-2 hours

**Finding**: Hot path functions may not be inlined

**Implementation**:
```rust
// Add #[inline] to hot functions:
#[inline]
pub fn get(&self, key: &[u8]) -> Option<Value> { ... }

#[inline]
fn partition_for_key(key: &[u8]) -> usize { ... }
```

**Files to change**:
- `src/db.rs` - get(), put(), partition_for_key()
- `src/memtable.rs` - insert(), get()
- `src/sstable/block.rs` - decode functions

**Expected impact**: +1-2%

### 5. Reduce Allocations (Priority 5) ⏱️ 2-3 hours

**Finding**: May have unnecessary allocations in hot paths

**Areas to investigate**:
1. Key cloning - use references where possible
2. Temporary vectors - reuse buffers
3. String allocations - use &str

**Tools**:
```bash
cargo flamegraph --release --example baseline_benchmark
# Look for alloc/dealloc in flamegraph
```

**Expected impact**: +2-4%

## Implementation Strategy

### Phase 1: Quick Wins (Day 1)
1. ✅ Add dependencies (varint-rs, quick_cache) - DONE
2. Implement varint-rs replacement
3. Test and benchmark
4. **Expected**: +1-3%

### Phase 2: Cache Optimization (Day 2)
1. Implement quick_cache
2. Test and benchmark
3. **Expected**: +3-5% (cumulative: +4-8%)

### Phase 3: Compaction Tuning (Day 3)
1. Analyze current compaction behavior
2. Tune parameters
3. Test and benchmark
4. **Expected**: +5-10% (cumulative: +9-18%)

### Phase 4: Micro-optimizations (Day 4-5)
1. Add inline attributes
2. Profile and reduce allocations
3. Test and benchmark
4. **Expected**: +3-6% (cumulative: +12-24%)

## Success Criteria

**Minimum**: +10% improvement (473K → 520K ops/sec)
**Target**: +20% improvement (473K → 568K ops/sec)  
**Stretch**: +27% improvement (473K → 600K+ ops/sec, beat fjall!)

## Risk Mitigation

- Test after each change (incremental validation)
- Keep git commits small (easy to revert)
- Run full test suite between changes
- Benchmark consistently (same conditions)

## Measurement Protocol

**Before each change**:
```bash
# Clean benchmark (3 runs, take median)
for i in 1 2 3; do
  cargo run --release --features baseline-benchmarks --example baseline_benchmark
done
```

**After each change**:
```bash
# Same 3-run protocol
# Document: change, performance delta, commit hash
```

## Exit Criteria

**Ship if**:
- ✅ Reach 550K+ ops/sec mixed workload (+16%)
- ✅ All tests passing
- ✅ No regressions on other workloads

**Continue if**:
- Performance gains diminish (<2% per day)
- 5 days elapsed without reaching target
- Complexity increases unacceptably

## Timeline

**Day 1**: varint-rs (+1-3%)
**Day 2**: quick_cache (+3-5%)
**Day 3**: Compaction tuning (+5-10%)
**Day 4**: Inline + allocations (+3-6%)
**Day 5**: Final validation + documentation

**Total**: 5 days to +12-24% improvement

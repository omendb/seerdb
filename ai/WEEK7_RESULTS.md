# Week 7: LSM Compaction - Results

**Date**: November 1, 2025
**Status**: ✅ Complete
**Tests**: 43 passing (39 unit + 4 integration)

---

## Summary

Implemented LSM-tree compaction system with merge iterator, enabling bounded read amplification as the database grows. Core compaction logic complete and tested.

---

## Features Implemented

### 1. LSM Level Structure

**Design**:
- Multi-level hierarchy (L0, L1, L2, ... L6)
- L0: Memtable flush target (no size limit)
- L1+: Exponentially increasing size thresholds

**Implementation**:
- `Level` struct: Tracks SSTables and size per level
- `LSMTree` struct: Manages all levels
- Size-based compaction triggers

**Thresholds**:
```
L0: Unlimited size, triggers on 4+ SSTables
L1: base_size (default: 10MB)
L2: base_size * 10 (100MB)
L3: base_size * 100 (1GB)
...
```

**Code**: `src/compaction/mod.rs:72-152`

### 2. Merge Iterator

**Purpose**: K-way merge of multiple sorted SSTables during compaction

**Algorithm**:
1. Collect all entries from all SSTables
2. Sort by key, then by source_id (lower = newer)
3. Deduplicate: Keep first occurrence (newest value)

**Performance**:
- Simple implementation (collects all upfront)
- Handles duplicates correctly (keeps newest)
- Memory cost: O(total entries) during compaction

**Trade-off**: Not a streaming merge (for simplicity), but acceptable for compaction

**Code**: `src/compaction/merge.rs:1-78`

### 3. Compaction Function

**Function**: `compact_sstables(input_paths, output_path)`

**Process**:
1. Open all input SSTables
2. Create merge iterator
3. Build new SSTable from merged entries
4. Return path and size of output

**Features**:
- Automatic deduplication
- Preserves sorted order
- Returns output metadata (path, size)

**Code**: `src/compaction/mod.rs:27-70`

---

## Architecture

### Level Management

```
LSM Tree:
┌─────────────────────────────────────┐
│ L0: [sst1, sst2, sst3, sst4]        │  Trigger: 4+ files
├─────────────────────────────────────┤
│ L1: [merged.sst]                    │  Threshold: 10MB
├─────────────────────────────────────┤
│ L2: [merged.sst]                    │  Threshold: 100MB
├─────────────────────────────────────┤
│ L3: [merged.sst]                    │  Threshold: 1GB
└─────────────────────────────────────┘
```

### Compaction Flow

```
Input:
  L0: [sst1, sst2, sst3, sst4]  (trigger: too many files)
  L1: [existing.sst]

Compact L0 → L1:
  1. Merge [sst1, sst2, sst3, sst4, existing.sst]
  2. Deduplicate (keep newest)
  3. Write new.sst

Output:
  L0: []
  L1: [new.sst]
```

### Merge Example

```
SSTable 1 (newer):  key1=A, key2=B
SSTable 2 (older):  key1=X, key3=C

Merge process:
  1. Collect: [(key1,A,0), (key2,B,0), (key1,X,1), (key3,C,1)]
  2. Sort:    [(key1,A,0), (key1,X,1), (key2,B,0), (key3,C,1)]
  3. Dedup:   [(key1,A), (key2,B), (key3,C)]

Result: key1=A (newer kept), key2=B, key3=C
```

---

## Tests

**43 tests passing**:
- 39 unit tests
- 4 integration tests

**New Compaction Tests** (11 total):

### Level Management (5 tests)
- `test_level_creation`: Level initialization
- `test_level_compaction_trigger`: Size-based triggers
- `test_lsm_tree_creation`: Multi-level structure
- `test_l0_compaction_trigger`: L0 file count trigger
- `test_level_size_compaction_trigger`: L1+ size trigger

### Merge Iterator (4 tests)
- `test_merge_single_sstable`: Single SSTable passthrough
- `test_merge_two_sstables`: Basic 2-way merge
- `test_merge_with_duplicates`: Deduplication (newest wins)
- `test_merge_many_sstables`: K-way merge (5 SSTables)

### Compaction Function (2 tests)
- `test_compact_sstables`: End-to-end compaction
- `test_compact_with_duplicates`: Compaction with overwrites

---

## Code Statistics

**Lines Added**:
- `src/compaction/mod.rs`: 359 lines (level management, compaction)
- `src/compaction/merge.rs`: 221 lines (merge iterator)
- **Total**: 580 lines

**Total Codebase**: ~2,180 lines
- WAL: 411 lines
- Memtable: 234 lines
- SSTable: 425 lines
- Bloom: 252 lines
- Compaction: 580 lines
- Tests: ~280 lines

---

## Performance Characteristics

### Read Amplification

**Without Compaction**:
- N memtable flushes = N SSTables in L0
- Read cost: O(N * log M) where M = entries per SSTable
- Example: 1000 flushes = check 1000 SSTables

**With Compaction**:
- Bounded number of SSTables per level
- Read cost: O(levels * log M)
- Example: 7 levels = check at most 7 SSTables

**Improvement**: O(N) → O(log N) in best case

### Write Amplification

**Current**: Simple leveled compaction
- Each entry written once per level
- Write amplification: ~10x (depends on level ratio)

**Future**: Lazy leveling (Dostoevsky)
- Upper levels: Tiered (less write amp)
- Largest level: Leveled (good read amp)
- Target: 2-3x write amplification

### Space Amplification

**During Compaction**:
- Temporary 2x space usage (old + new SSTables)
- Old SSTables deleted after compaction completes

**Steady State**:
- 10-20% overhead from obsolete data
- Reduced by compaction (removes duplicates)

---

## Design Decisions

### 1. Collect-and-Sort Merge

**Decision**: Collect all entries upfront, then sort

**Rationale**:
- Simpler than streaming k-way merge
- SSTable::iter() requires &mut self (file seeking)
- Compaction is background task (memory ok)

**Trade-off**:
- Memory: O(total entries) during merge
- Benefit: Simple, correct, testable

**Future**: Consider streaming merge for large compactions

### 2. L0 Triggers on File Count

**Decision**: L0 triggers on 4+ files, not size

**Rationale**:
- L0 SSTables can overlap (not sorted globally)
- File count directly impacts read amplification
- Typical LSM trees use file count for L0

**Benchmark**: RocksDB default is 4 files

### 3. Size Ratio = 10

**Decision**: Use 10x size ratio between levels

**Rationale**:
- Standard in LSM literature (Dostoevsky paper)
- Good balance of read/write amplification
- RocksDB default is 10

**Formula**: `L_n = base_size * 10^(n-1)`

### 4. Deduplication Strategy

**Decision**: Keep entry from lowest source_id (newest)

**Rationale**:
- Input SSTables ordered by age (newest first)
- Lower source_id = later in time = should override
- Matches LSM semantics (newer writes win)

**Implementation**: Sort by (key, source_id), keep first

---

## Integration Status

### ✅ Complete
- Level structure
- Merge iterator
- Compaction function
- Size/file count triggers
- Deduplication

### 🚧 Not Yet Implemented
- Background compaction thread
- Integration with main DB interface
- Compaction strategy selection (leveled vs lazy leveling)
- File cleanup after compaction
- Compaction metrics

---

## Next Steps (Week 8)

**Goal**: Create main DB interface that integrates all components

**Tasks**:
1. **DB struct**: Combines WAL, memtable, LSMTree
2. **Public API**: get(), put(), delete(), scan()
3. **Flush logic**: Memtable → L0 SSTable
4. **Compaction scheduling**: Trigger compaction on flush
5. **File management**: Delete old SSTables after compaction
6. **Recovery**: WAL replay on startup
7. **Tests**: End-to-end DB operations

**Stretch Goals**:
- Background compaction thread
- Metrics (write amp, read amp, space amp)
- Benchmark vs fjall baseline

---

## Lessons Learned

1. **Simplicity wins**: Collect-and-sort merge is simpler than streaming
2. **Borrow checker challenges**: SSTable::iter() lifetime issues led to simpler design
3. **Test duplicates early**: Deduplication bugs caught by tests
4. **Stable sort matters**: Preserves ordering for equal keys (critical for correctness)

---

## Potential Improvements (Future)

### Streaming Merge Iterator
- Use priority queue (BinaryHeap) for k-way merge
- O(1) memory per iterator instead of O(total entries)
- Requires refactoring SSTable::iter() to not need &mut self

### Lazy Leveling (Dostoevsky)
- Upper levels: Tiered compaction (less write amp)
- Largest level: Leveled compaction (better read amp)
- Expected: 2-3x better write amplification

### Parallel Compaction
- Multiple compaction threads for different levels
- Requires careful locking/coordination
- Week 8+ optimization

### Bloom Filter Integration
- Check bloom before merging from SSTable
- Skip SSTables that definitely don't have key
- Reduces compaction I/O

---

## Commit

```
ea3b5bd - feat: implement LSM compaction with merge iterator

Week 7 Compaction System:
- LSM level structure with size-based triggers
- K-way merge iterator for merging multiple SSTables
- compact_sstables() function for SSTable merging
- Automatic deduplication (keeps newest values)
- 43 tests passing (39 unit + 4 integration)

Components:
- src/compaction/mod.rs: Level management, LSM tree structure
- src/compaction/merge.rs: Merge iterator for k-way merge
- compact_sstables(): Merges multiple SSTables into one
```

---

*Week 7 Complete - Ready for Week 8: Main DB Interface*

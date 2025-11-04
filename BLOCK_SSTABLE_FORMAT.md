# Block-Based SSTable Format (V2)

## Overview

This document specifies the new block-based SSTable format designed for memory efficiency with large datasets.

## Problem Statement

**Current Format** (V1): Loads entire key index into RAM
- 10M keys × 26 bytes = 260 MB per SSTable
- With 20 SSTables open: 5.2 GB of memory
- Unacceptable for large datasets

**New Format** (V2): Block-based index with lazy loading
- Only top-level index in RAM: ~200 KB per SSTable
- Load index/data blocks on-demand
- **99% memory reduction**

## File Structure

```
┌─────────────────────────────────────────────────────────────┐
│ File Header (32 bytes)                                       │
│  magic: u32 (0x53535442 = "SSTB")                          │
│  version: u32 (0x00000002 = V2)                             │
│  flags: u64 (reserved)                                       │
│  num_entries: u64                                            │
│  reserved: u64                                               │
├─────────────────────────────────────────────────────────────┤
│ Data Blocks (4KB each)                                       │
│  ┌──────────────────────────────────────┐                   │
│  │ Block 0:                             │                   │
│  │  Entry 0: [key_len][key][flag][value_or_pointer]       │
│  │  Entry 1: [key_len][key][flag][value_or_pointer]       │
│  │  ...                                  │                   │
│  │  Restart Points: [offset_0]...[offset_N]               │
│  │  Num Restarts: u32                    │                   │
│  │  Checksum: u32                         │                   │
│  └──────────────────────────────────────┘                   │
│  Block 1: ...                                                │
│  Block N: ...                                                │
├─────────────────────────────────────────────────────────────┤
│ Index Blocks (4KB each)                                      │
│  ┌──────────────────────────────────────┐                   │
│  │ Index Block 0:                        │                   │
│  │  Entry 0: [key_len][key][block_offset: u64]            │
│  │  Entry 1: [key_len][key][block_offset: u64]            │
│  │  ...                                  │                   │
│  │  (Last key of each data block + offset)                │
│  │  Restart Points: [offset_0]...[offset_N]               │
│  │  Num Restarts: u32                    │                   │
│  │  Checksum: u32                         │                   │
│  └──────────────────────────────────────┘                   │
│  Index Block 1: ...                                          │
│  Index Block M: ...                                          │
├─────────────────────────────────────────────────────────────┤
│ Top-Level Index (variable size, loaded into RAM)            │
│  Num Index Blocks: u32                                       │
│  For each index block:                                       │
│    [last_key_len: u32][last_key: bytes][offset: u64]       │
├─────────────────────────────────────────────────────────────┤
│ Bloom Filter                                                 │
│  Length: u64                                                 │
│  Data: [bytes]                                               │
├─────────────────────────────────────────────────────────────┤
│ Footer (48 bytes)                                            │
│  index_blocks_offset: u64                                    │
│  top_level_index_offset: u64                                 │
│  bloom_offset: u64                                           │
│  checksum: u32 (CRC32 of entire file except this field)     │
│  magic: u32 (0x53535442 = "SSTB")                           │
│  version: u32 (0x00000002 = V2)                             │
│  reserved: u32                                               │
└─────────────────────────────────────────────────────────────┘
```

## Entry Format

```
[key_len: u32][key: bytes][flag: u8][value_data]

flag = 0x00: Inline value
  value_data = [value_len: u32][value: bytes]

flag = 0x01: VLog pointer
  value_data = [vlog_offset: u64][vlog_length: u32]
```

## Lookup Algorithm

### Point Lookup: Get(key)

1. **Bloom Filter Check** (in RAM)
   - If not present → return None
   - If present → continue (might be false positive)

2. **Top-Level Index Binary Search** (in RAM)
   - Find index block that might contain key
   - Load index block from disk (or cache)

3. **Index Block Binary Search** (loaded on-demand)
   - Find data block that contains key
   - Load data block from disk (or cache)

4. **Data Block Binary Search** (loaded on-demand)
   - Find exact key in block
   - Return value (or load from vlog if pointer)

### Scan: Range(start_key, end_key)

1. Use top-level index to find first relevant index block
2. Load index blocks sequentially
3. For each relevant data block:
   - Load block
   - Iterate entries in range
   - Yield matching entries

## Memory Usage Analysis

### V1 (Current)
```
Per SSTable (1M entries):
- Full Index: 1M × (26 bytes key + 8 bytes offset) = 34 MB
- Bloom Filter: ~1.2 MB
- Total per SSTable: ~35 MB

With 20 SSTables open: 700 MB
```

### V2 (Block-Based)
```
Per SSTable (1M entries):
- Top-Level Index: ~250 index blocks × (26 bytes + 8 bytes) = 8.5 KB
- Bloom Filter: ~1.2 MB
- Total in RAM: ~1.21 MB

With 20 SSTables open: ~24 MB
Block Cache (LRU): 32 MB (configured)
Total: ~56 MB

Memory Reduction: 700 MB → 56 MB = **92% reduction**
```

## Implementation Strategy

### Phase 1: Core Format (This Session)
- [x] Block module (BlockBuilder, Block, BlockIterator)
- [ ] New SSTableBuilder with block-based writing
- [ ] New SSTable with lazy block loading
- [ ] Update all existing tests

### Phase 2: Performance (Next Session)
- [ ] LRU cache for blocks
- [ ] Benchmark vs V1
- [ ] Optimize hot paths

### Phase 3: Production (Future)
- [ ] mmap support for zero-copy
- [ ] Compression (snappy/zstd)
- [ ] Prefix compression in blocks

## Backward Compatibility

**Breaking Change**: V2 format is incompatible with V1

Migration strategy:
1. Detect format version from footer magic/version
2. V1 reader still available (keep old code)
3. Write helper to convert V1 → V2 (optional)

For pre-1.0 software, breaking compatibility is acceptable for major improvements.

## Testing Strategy

1. **Unit Tests**: Block read/write, index operations
2. **Integration Tests**: Full SSTable lifecycle with blocks
3. **Memory Tests**: Verify <10% memory growth with 10GB dataset
4. **Performance Tests**: Compare V1 vs V2 throughput/latency
5. **Stress Tests**: 100GB+ datasets with limited RAM

## Success Criteria

- ✅ All existing tests pass with new format
- ✅ Memory usage: <100 MB for 20 SSTables (vs 700 MB in V1)
- ✅ Performance: Within 10% of V1 for point lookups
- ✅ Performance: 2-3x faster for scans (better cache locality)
- ✅ 10GB soak test passes with <150 MB total memory

## References

- RocksDB Block Format: https://github.com/facebook/rocksdb/wiki/Rocksdb-BlockBasedTable-Format
- LevelDB Table Format: https://github.com/google/leveldb/blob/main/doc/table_format.md
- WiscKey Paper: https://www.usenix.org/system/files/conference/fast16/fast16-papers-lu.pdf

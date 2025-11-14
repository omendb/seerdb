# Storage Format Decisions

**Format**: Decision → Rationale → Trade-offs → References

---

## SSTable Binary Search with Full Key Index (Nov 1, 2025)

**Decision**: Store full keys in SSTable index, not just offsets

**Rationale**:
- Enables O(log n) binary search directly on keys
- Previous: Vec<u64> (offsets only) → O(n) linear scan
- Current: Vec<(Bytes, u64)> (key + offset) → O(log n) binary search

**Implementation**:
- Index: Vec<(Bytes, u64)> stored in SSTable
- Search: binary_search_by() on sorted keys
- Memory cost: ~1-2 MB per SSTable (acceptable)

**Trade-offs**:
- ✅ Binary search: O(log n) lookups
- ✅ 100k entries: 17 comparisons vs 100k comparisons
- ❌ Memory: ~1-2 MB index per SSTable

**Performance**: 476k ops/sec existing keys, 9.1M ops/sec missing keys (19x speedup from bloom)

**Commits**: a4d2c8b

---

## Bloom Filter Integration (Nov 1, 2025)

**Decision**: Check bloom filter before binary search

**Rationale**:
- Eliminates unnecessary lookups for missing keys
- 19x speedup for negative lookups (192x at 100k scale)
- 1% FPR = 99% of missing keys filtered instantly

**Implementation**:
- Bloom filter built during SSTable construction
- Serialized to SSTable file (footer: [index_offset][bloom_offset])
- Checked in get() before binary search

**Trade-offs**:
- ✅ Missing keys: ~11 µs constant (regardless of SSTable size)
- ✅ Space: 122 KB for 100k keys (1% FPR)
- ❌ 1% false positives still do binary search + disk read
- ✅ 99% benefit outweighs 1% cost

**Performance**: 100k entries, missing key lookups 192x faster than without bloom

**Commits**: a4d2c8b

---

## Collect-and-Sort Merge Strategy (Nov 1, 2025)

**Decision**: Collect all entries upfront, then sort (not streaming k-way merge)

**Rationale**:
- SSTable::iter() requires &mut self (file seeking)
- Streaming k-way merge with BinaryHeap has lifetime issues
- Compaction is background task (memory acceptable)
- Simplicity > streaming efficiency

**Implementation**:
```rust
// Collect all entries from all SSTables
for sstable in sstables {
    entries.extend(sstable.iter());
}
// Sort by (key, source_id)
entries.sort_by(|(k1, sid1, _), (k2, sid2, _)|
    k1.cmp(k2).then(sid1.cmp(sid2))
);
// Deduplicate: keep first (newest)
```

**Trade-offs**:
- ✅ Simple, correct, testable
- ✅ Easier to reason about deduplication
- ❌ Memory: O(total entries) during merge
- ❌ Not streaming (but acceptable for compaction)

**Future**: Consider streaming merge if large compactions become bottleneck

**Commits**: ea3b5bd

---

## Deduplication Strategy: Newest Wins (Nov 1, 2025)

**Decision**: Keep entry from lowest source_id (newest value)

**Rationale**:
- Input SSTables ordered by age (newest first)
- Lower source_id = later in time = should override
- Matches LSM semantics (newer writes win)

**Implementation**:
- Sort by (key, source_id)
- Stable sort preserves ordering
- Keep first occurrence after sort

**Trade-offs**:
- ✅ Correct LSM semantics
- ✅ Simple: just stable sort + dedup
- ✅ Handles overwrites, deletes (tombstones)

**Commits**: ea3b5bd

---

## SSTable Metadata for Range Filtering (Nov 7, 2025)

**Decision**: Add min_key/max_key metadata to SSTables for range query optimization

**Problem**: Range scans were 95% slower than RocksDB - creating iterators for ALL SSTables

**Solution**: Track key range bounds in SSTable metadata

**Format Change**: SSTable v1 format
- Footer: 40 bytes → 48 bytes (added metadata_offset)
- Metadata section: min_key length + min_key + max_key length + max_key
- Backward incompatible (v0 → v1, acceptable at 0.0.x)

**Results**:
- **Range scans**: 870 → 17,087 scans/sec (19.6x improvement!)
- **Ratio vs RocksDB**: 0.04x → 0.81x (competitive!)
- **Time per scan**: 1,148µs → 58µs (20x faster)

**How It Works**:
```
Query: range [key_00100, key_00200)
SSTable A: [key_00000, key_00050)  → SKIP (no overlap)
SSTable B: [key_00100, key_00150)  → INCLUDE (overlaps)
SSTable C: [key_00250, key_00300)  → SKIP (no overlap)
Result: Create only 1 iterator instead of 3
```

**Trade-offs**:
- ✅ 19.6x range scan improvement
- ✅ Minimal overhead (8 bytes + 2 key lengths in footer)
- ❌ Backward incompatible format change (v0 → v1)

**Commits**: 5e4dc0c

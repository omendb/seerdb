# seerdb Block Cache Optimization

**Priority**: HIGH - Critical for omendb disk search performance
**Date**: November 17, 2025

## Problem

omendb's disk search performance is 27x slower than in-memory (22 QPS vs 597 QPS). Root cause: every `prefix()` scan reads SSTable blocks from disk. No block caching exists.

## Current State

```rust
// db.rs line 849
sstable_cache: Arc::new(Cache::new(1000)), // Caches SSTable metadata only
```

seerdb currently caches:
- ✅ SSTable objects (file handles, metadata)
- ❌ **Data blocks** (actual key-value data)
- ❌ **Index blocks** (bloom filters, block indexes)

## Proposed Solution

### 1. Add Block Cache (HIGH PRIORITY)

Add LRU cache for SSTable data blocks using `quick_cache`:

```rust
// In DBOptions
pub struct DBOptions {
    // ... existing fields ...

    /// Block cache capacity in bytes
    ///
    /// Caches SSTable data blocks for faster reads.
    /// Higher values improve read performance but use more memory.
    ///
    /// Default: 64MB
    /// Recommended: 10-25% of available RAM
    pub block_cache_capacity: usize,
}

impl Default for DBOptions {
    fn default() -> Self {
        Self {
            // ... existing ...
            block_cache_capacity: 64 * 1024 * 1024, // 64MB
        }
    }
}
```

```rust
// In DB struct
pub struct DB {
    // ... existing ...

    /// Block cache (key: sstable_id + block_offset, value: block data)
    block_cache: Arc<Cache<(u64, u64), Bytes>>,
}
```

### 2. Cache Key Design

```rust
/// Cache key for data blocks
struct BlockCacheKey {
    sstable_id: u64,  // Unique SSTable identifier
    block_offset: u64, // Block offset within file
}
```

### 3. Integration Points

**SSTable::read_block()** - Add cache lookup:

```rust
impl SSTable {
    fn read_block(&self, offset: u64, cache: &Cache<...>) -> Result<Block> {
        let key = (self.id, offset);

        // Check cache first
        if let Some(block) = cache.get(&key) {
            return Ok(block);
        }

        // Cache miss - read from disk
        let block = self.read_block_from_disk(offset)?;

        // Insert into cache (uses weighted capacity based on block size)
        cache.insert(key, block.clone(), block.len() as u32);

        Ok(block)
    }
}
```

### 4. Prefix Scan Optimization

For HNSW edge storage, the pattern is:
- Prefix: `node_id (8B) || level (1B)`
- Scan: All keys with this prefix

This will naturally benefit from block cache because:
- Same SSTable blocks accessed repeatedly for hot nodes
- Popular nodes (high-level, frequently visited) get cached
- Sequential access within blocks is cache-friendly

### 5. Performance Impact

**Expected improvement for omendb:**

| Metric | Before | After (est.) | Improvement |
|--------|--------|--------------|-------------|
| Disk Search | 22 QPS | 200-400 QPS | 10-20x |
| L0 Search | 597 QPS | 597 QPS | (no change) |
| Gap (L0 vs Disk) | 27x | 1.5-3x | Acceptable |

**Why this works:**
- HNSW traversal visits ~100-200 nodes per query
- Each node lookup reads 1-2 blocks
- With 64MB cache and 4KB blocks, can cache ~16K blocks
- Popular nodes (hot set) fit entirely in cache

### 6. Configuration Recommendations

```rust
// For omendb (graph edge storage):
DBOptions {
    block_cache_capacity: 256 * 1024 * 1024, // 256MB for graph workload
    memtable_capacity: 64 * 1024 * 1024,     // 64MB memtable
    ..Default::default()
}

// General LSM workload:
DBOptions {
    block_cache_capacity: 64 * 1024 * 1024,  // 64MB default
    ..Default::default()
}
```

### 7. Implementation Steps

1. Add `block_cache_capacity` to `DBOptions`
2. Add `block_cache` field to `DB` struct (Arc<Cache>)
3. Add `id` field to `SSTable` struct for cache key
4. Modify `SSTable::read_block()` to check cache first
5. Add cache miss/hit metrics to `DBStats`
6. Add tests for cache behavior
7. Benchmark before/after

### 8. Memory Overhead

- Cache metadata: ~100 bytes per block entry
- Block data: Variable (typically 4-16KB)
- Total: ~10% overhead over cached data size

For 64MB cache with 4KB blocks:
- ~16,384 blocks cached
- ~1.6MB metadata overhead
- Effective capacity: ~62MB

### 9. Alternative: Row Cache (Lower Priority)

Row cache (caches individual key-value results) is less effective for prefix scans:
- Prefix scans return multiple keys
- Cache invalidation is complex
- Block cache naturally handles this

**Recommendation**: Start with block cache, add row cache later if needed.

---

## Files to Modify

1. `src/db.rs` - Add DBOptions field, DB field
2. `src/sstable/mod.rs` - Add cache lookup in block reads
3. `src/sstable/block.rs` - Block serialization for cache
4. `src/metrics.rs` - Add cache hit/miss counters

## Testing

1. Unit test: Cache hit/miss behavior
2. Benchmark: Read performance with cache enabled
3. Integration: omendb disk search performance

## Success Criteria

- Disk search: 22 QPS → 200+ QPS (10x improvement)
- Cache hit rate: >80% for hot workloads
- Memory overhead: <10% over cache size

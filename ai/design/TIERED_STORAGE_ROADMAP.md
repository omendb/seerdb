# Tiered Storage Roadmap

**Last Updated**: November 9, 2025
**Status**: Post-0.0.1 Feature (0.0.2+)
**Priority**: Medium-High (enables billion-scale deployments)

---

## Executive Summary

**Goal**: Enable RAM → SSD → S3 tiered storage for LSM-tree levels

**Why**: Cost optimization at billion-scale workloads
- **Current**: All data on single storage tier (local disk)
- **Future**: Hot data (L0-L2) on SSD, cold data (L3-L6) on S3
- **Impact**: 3-5x cost reduction for large deployments (100M-1B+ keys)

**Timeline**: Post-0.0.1 (after production hardening complete)

---

## Storage Hierarchy Design

### Target Architecture

```
┌─────────────────────────────────────────────────────┐
│ LSM-Tree Tiered Storage                            │
├─────────────────────────────────────────────────────┤
│                                                      │
│  L0 (RAM)                                          │
│  ├── Memtable + WAL                                │
│  ├── Write buffer                                  │
│  └── Latency: <100μs                               │
│                                                      │
│  L1-L2 (Local SSD) - Warm Tier                     │
│  ├── Recently compacted SSTables                   │
│  ├── Frequently accessed blocks                    │
│  ├── Storage: Local NVMe/SSD                       │
│  └── Latency: 100μs-5ms                            │
│                                                      │
│  L3-L6 (Object Storage) - Cold Tier                │
│  ├── Older SSTables (infrequent access)            │
│  ├── Storage: S3/GCS/Azure Blob                    │
│  ├── Caching: Prefetch hot blocks to SSD           │
│  └── Latency: 50-200ms                             │
│                                                      │
└─────────────────────────────────────────────────────┘
```

### Cost Comparison (600GB total data)

| Tier | Storage | Cost/GB | Monthly | Latency | % Data |
|------|---------|---------|---------|---------|--------|
| **RAM** | DRAM | $5.00 | $3,000 | <100μs | 1% (memtable) |
| **SSD** | Local NVMe | $0.10 | $60 | 1-5ms | 9% (L1-L2) |
| **S3** | Object storage | $0.023 | $14 | 50-200ms | 90% (L3-L6) |

**All SSD**: $60/mo, **Tiered**: ~$45/mo (25% savings at 600GB, grows with scale)

---

## Technical Requirements

### 1. Storage Backend Abstraction

**Goal**: Pluggable storage backends (local disk, S3, GCS, Azure) with zero-cost abstraction

**Status**: ✅ **IMPLEMENTED** (0.0.1) - Feature-gated trait with zero overhead

**Interface**:
```rust
/// Storage trait for pluggable storage implementations (feature-gated)
///
/// Zero-Cost Abstraction:
/// - When `s3-backend` feature is disabled (default): No trait overhead, direct calls
/// - When `s3-backend` feature is enabled: Monomorphized generics (static dispatch)
#[cfg(feature = "s3-backend")]
pub trait Storage: Send + Sync {
    /// Read a block from an SSTable at the given offset
    fn read_block(&self, path: &Path, offset: u64, size: u32) -> Result<Vec<u8>>;

    /// Write an SSTable to storage
    fn write_sstable(&self, path: &Path, data: &[u8]) -> Result<()>;

    /// Delete an SSTable from storage
    fn delete_sstable(&self, path: &Path) -> Result<()>;

    /// Fsync an SSTable to ensure durability
    fn sync(&self, path: &Path) -> Result<()>;

    /// Check if an SSTable exists
    fn exists(&self, path: &Path) -> Result<bool>;

    /// List all SSTables in a directory
    fn list_sstables(&self, dir: &Path) -> Result<Vec<PathBuf>>;
}
```

**Implementations**:
```rust
// Phase 1: Local disk (IMPLEMENTED - 0.0.1)
pub struct LocalStorage {
    base_path: PathBuf,
}

// Phase 2: Object storage via object_store crate (post-0.0.1)
pub struct ObjectStorage<T: ObjectStore> {
    store: Arc<T>,  // S3, GCS, Azure, etc.
    cache: Arc<Cache<String, Vec<u8>>>,
}
```

**Design Decision**: Use `object_store` crate (Apache Arrow project)
- ✅ Battle-tested multi-cloud abstraction
- ✅ Supports S3, GCS, Azure Blob, Cloudflare R2
- ✅ Async I/O with connection pooling
- ✅ Maintained by Apache Foundation

### 2. Tiering Policy

**Goal**: Automatic tier assignment based on LSM level

**Configuration**:
```rust
pub struct TieringConfig {
    /// Levels stored on local disk (typically L0-L2)
    pub hot_levels: Vec<u8>,

    /// Levels stored on object storage (typically L3-L6)
    pub cold_levels: Vec<u8>,

    /// Prefetch hot blocks from cold storage
    pub prefetch_enabled: bool,

    /// Cache size for object storage blocks (bytes)
    pub cold_cache_size: usize,
}

impl Default for TieringConfig {
    fn default() -> Self {
        Self {
            hot_levels: vec![0, 1, 2],  // L0-L2 on SSD
            cold_levels: vec![3, 4, 5, 6],  // L3-L6 on S3
            prefetch_enabled: true,
            cold_cache_size: 1_000_000_000,  // 1GB cache
        }
    }
}
```

### 3. Compaction Integration

**Goal**: Tier-aware compaction (move data between tiers)

**Behavior**:
```rust
// Compaction output tier selection
impl Compaction {
    fn select_output_tier(&self, output_level: u8) -> &dyn StorageBackend {
        if self.config.hot_levels.contains(&output_level) {
            &self.hot_storage  // Write to SSD
        } else {
            &self.cold_storage  // Write to S3
        }
    }
}
```

**Edge Cases**:
- Compacting L2 → L3: Read from SSD, write to S3
- Compacting L5 → L6: Read from S3, write to S3 (stay cold)
- Cache warming: Prefetch L3 SSTables if frequently accessed

### 4. Block Cache Integration

**Goal**: Unified cache for both SSD and S3 blocks

**Architecture**:
```rust
pub struct TieredBlockCache {
    /// Hot tier cache (SSD blocks, smaller)
    hot_cache: Arc<Cache<BlockId, Block>>,

    /// Cold tier cache (S3 blocks, larger)
    cold_cache: Arc<Cache<BlockId, Block>>,
}

impl TieredBlockCache {
    pub fn get(&self, block_id: BlockId, tier: Tier) -> Option<Block> {
        match tier {
            Tier::Hot => self.hot_cache.get(&block_id),
            Tier::Cold => self.cold_cache.get(&block_id)
                .or_else(|| {
                    // Cache miss: fetch from S3
                    let block = self.fetch_from_s3(block_id)?;
                    self.cold_cache.insert(block_id, block.clone());
                    Some(block)
                }),
        }
    }
}
```

### 5. Prefetching Strategy

**Goal**: Reduce S3 latency via predictive prefetching

**Strategies**:
1. **Sequential prefetch**: During range scans, prefetch next N blocks
2. **Bloom filter hints**: Prefetch blocks likely to contain key
3. **Compaction prefetch**: Warm cache before compaction starts

**Implementation**:
```rust
pub struct PrefetchPolicy {
    /// Number of blocks to prefetch ahead
    pub sequential_distance: usize,

    /// Prefetch blocks with >X% bloom filter match probability
    pub bloom_threshold: f64,

    /// Background thread pool for async prefetch
    pub prefetch_workers: usize,
}
```

---

## Implementation Phases

### Phase 1: Storage Abstraction ✅ **COMPLETED** (0.0.1)

**Tasks**:
- [x] Define `Storage` trait (feature-gated for zero overhead)
- [x] Implement `LocalStorage` with dual implementation (trait + standalone)
- [x] Add `s3-backend` feature flag to Cargo.toml
- [x] Verify zero overhead with storage tests (4/4 passing)
- [x] Update `SSTable` to use `LocalStorage` for reads (completed - all tests passing)
- [ ] Update `Compaction` to use `LocalStorage` (pending - Phase 2)
- [ ] Add configuration for backend selection (pending - Phase 2)

**Note**: SSTableBuilder still uses File directly for streaming writes (write header,
blocks incrementally, footer). This is intentional - object storage backends require
buffering all data before upload. Will be refactored in Phase 2 when adding S3 support.

**Deliverable**: ✅ Pluggable storage foundation with zero-cost abstraction
**Location**: `src/storage.rs` (290 LOC), `src/sstable/mod.rs` (refactored reads)
**Test Status**: ✅ 68 tests passing, 0 failures (verified after refactoring)

### Phase 2: Object Storage Backend (Week 3-4, ~600 LOC)

**Tasks**:
- [ ] Integrate `object_store` crate (multi-cloud abstraction)
- [ ] Implement `ObjectStorage<T: ObjectStore>` generic wrapper
- [ ] Add retry logic (transient failures)
- [ ] Handle rate limits (exponential backoff)
- [ ] Test with LocalFileSystem, InMemory, S3 backends

**Deliverable**: Functional object storage backend (S3, GCS, Azure, R2)

### Phase 3: Tiering Policy (Week 5, ~300 LOC)

**Tasks**:
- [ ] Add `TieringConfig` to `DBConfig`
- [ ] Implement tier selection logic
- [ ] Update compaction to respect tier policy
- [ ] Add tier migration (L2 → L3 moves SSD → S3)

**Deliverable**: Automatic tier assignment based on level

### Phase 4: Cold Tier Caching (Week 6-7, ~500 LOC)

**Tasks**:
- [ ] Implement `TieredBlockCache`
- [ ] Add LRU eviction for S3 blocks
- [ ] Integrate with existing block cache
- [ ] Add cache hit/miss metrics

**Deliverable**: LRU cache for S3 blocks, reduces latency

### Phase 5: Prefetching (Week 8-9, ~400 LOC)

**Tasks**:
- [ ] Implement sequential prefetch (range scans)
- [ ] Add background prefetch workers
- [ ] Implement bloom filter hinting
- [ ] Add prefetch metrics (cache hit rate)

**Deliverable**: Predictive prefetching reduces S3 latency

### Phase 6: Testing & Validation (Week 10, ~0 LOC)

**Tasks**:
- [ ] Integration tests (SSD + S3)
- [ ] Failure injection (S3 unavailable, network issues)
- [ ] Performance benchmarks (latency, cost)
- [ ] Long-running stability tests

**Deliverable**: Production-ready tiered storage

---

## Performance Targets

### Latency

| Operation | Local Disk | Tiered (Hot) | Tiered (Cold) |
|-----------|------------|--------------|---------------|
| **Point query** | 1-5ms | 1-5ms | 50-100ms (cold) / 1-5ms (cached) |
| **Range scan** | 5-20ms | 5-20ms | 100-500ms (cold) / 5-20ms (cached) |
| **Write** | 1-10ms | 1-10ms | 1-10ms (buffered) |

**Goal**: 90%+ cache hit rate on S3 blocks → most queries feel like local disk

### Cost

| Scale | All SSD | Tiered (10% SSD, 90% S3) | Savings |
|-------|---------|---------------------------|---------|
| **100GB** | $10/mo | $8/mo | 20% |
| **1TB** | $100/mo | $35/mo | 65% |
| **10TB** | $1,000/mo | $260/mo | 74% |

**Goal**: 50-75% cost reduction at multi-TB scale

---

## Consumer Integration Requirements

### Example Consumer Use Cases

**Graph-Based Applications** (connectivity data):
```rust
// Edges stored as keys in LSM-tree
EdgeKey { node_id, level, neighbor_id } → []
```

**Requirements**:
- ✅ Supports range scans (get all neighbors at level)
- ✅ Immutable SSTables (perfect for S3)
- ✅ Tiering works transparently (consumers don't need to know about S3)

**Time-Series Applications** (dense arrays):
```rust
// Metrics stored as values
timestamp → f64[N]
```

**Requirements**:
- May use separate storage (memmap files)
- OR use vLog with tiering (future optimization)

### Abstraction Boundaries

**What seerdb provides**:
- LSM-tree with pluggable storage backends
- Automatic tiering (L0-L2 SSD, L3-L6 S3)
- Block cache + prefetching for S3

**What consumers provide**:
- Key encoding (application-specific)
- Query operations (point/range/prefix)
- Compaction policies (optional)

**Clean separation**: seerdb handles storage tiers, consumers handle application logic

---

## Alternative Approaches Considered

### Option 1: RocksDB-Cloud

**Pros**:
- ✅ Production-proven (Rockset uses it)
- ✅ Remote compaction support
- ✅ Built-in S3 integration

**Cons**:
- ❌ C++ FFI (adds complexity)
- ❌ Loses Rust safety guarantees
- ❌ Not pure seerdb (dependency on RocksDB)

**Decision**: Build native Rust solution, can fall back to RocksDB-Cloud if needed

### Option 2: object_store Crate ✅ **SELECTED**

**Pros**:
- ✅ Multi-cloud abstraction (S3, GCS, Azure, Cloudflare R2)
- ✅ Well-maintained by Apache Arrow project
- ✅ Async I/O with connection pooling
- ✅ Retry logic and error handling built-in
- ✅ Simple, idiomatic Rust API

**Cons**:
- ⚠️ Adds dependency (~200KB compiled)

**Decision**: ✅ Use `object_store` crate (Phase 2)
- **Rationale**: Battle-tested, multi-cloud, maintained by Apache Foundation
- **Alternative**: DIY with `aws-sdk-s3` (more work, single cloud)
- **Trade-off**: Small dependency cost for huge maintainability win

### Option 3: Tiered at Application Layer (Not seerdb)

**Pros**:
- ✅ seerdb stays simple
- ✅ Application controls policy

**Cons**:
- ❌ Every consumer reimplements tiering
- ❌ Loses compaction integration
- ❌ No automatic tier migration

**Decision**: Build into seerdb (better abstraction, DRY principle)

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| S3 latency too high | Low | High | Aggressive caching + prefetching |
| S3 rate limits hit | Medium | Medium | Exponential backoff, request batching |
| Complex to implement | Medium | Medium | Incremental phases, test each |
| Cache eviction too aggressive | Low | Medium | Tunable cache size, metrics |
| Compaction slower (S3 I/O) | Medium | Medium | Remote compaction (future) |

---

## Success Criteria

### Functional Requirements

- [ ] ✅ Supports local disk backend (0.0.1)
- [ ] ✅ Supports S3 backend (0.0.2+)
- [ ] ✅ Automatic tier assignment (L0-L2 SSD, L3-L6 S3)
- [ ] ✅ Transparent to consumers (same API)
- [ ] ✅ Configurable tiering policy

### Performance Requirements

- [ ] ✅ 90%+ cache hit rate on S3 blocks
- [ ] ✅ <10ms p95 latency for cached S3 reads
- [ ] ✅ <200ms p99 latency for cold S3 reads
- [ ] ✅ No regression on all-SSD performance

### Cost Requirements

- [ ] ✅ 50-75% cost reduction at 1TB+ scale
- [ ] ✅ Configurable SSD/S3 ratio

---

## Timeline

**Post-0.0.1** (after production hardening complete):

- **Month 1-2**: Storage abstraction + S3 backend
- **Month 3**: Tiering policy + caching
- **Month 4**: Prefetching + testing
- **Total**: ~4 months (10 weeks of work)

**Prerequisite**: 0.0.1 shipped and stable (8 weeks)

---

## Related Work

### Papers

1. **"SpanDB: A Fast, Cost-Effective LSM-tree Based KV Store on Hybrid Storage"** (FAST 2021)
   - Tiered storage for LSM-trees (SSD + HDD)
   - Influenced our tiering policy design

2. **"RocksDB-Cloud: Enabling the Next Generation of Cloud-Native Databases"** (Rockset Blog)
   - Production deployment of LSM on S3
   - Validated feasibility of object storage tier

### Existing Implementations

- **RocksDB-Cloud**: S3 backend for RocksDB (C++, production)
- **fjall**: No tiering support (local disk only)
- **TiKV**: Tiered storage in Rust (PingCAP)

---

**Status**: Design complete, implementation post-0.0.1
**Next**: Ship 0.0.1 (production hardening), then build tiered storage
**Owner**: seerdb team

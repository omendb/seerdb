# seerdb Product Requirements

**Last Updated**: November 24, 2025
**Purpose**: Track what omendb and oadb need from seerdb

---

## Products Using seerdb

| Product | Description | seerdb Role |
|---------|-------------|-------------|
| **oadb** | Embedded vector DB | Vector metadata, graph edges, persistence |
| **OmenDB** | Cloud vector DB | HNSW edges, S3 tiering, graph storage |

---

## Current seerdb Capabilities

### What Works

| Feature | Status | Used By |
|---------|--------|---------|
| LSM-tree storage | ✅ | Both |
| WAL (crash recovery) | ✅ | Both |
| Compaction | ✅ | OmenDB |
| Bloom filters | ✅ | OmenDB |
| S3/object storage | ✅ (feature flag) | OmenDB |
| MVCC (snapshots) | ✅ | OmenDB |
| Partitioned memtable | ✅ | Both |
| SIMD optimizations | ✅ | Both |

### Feature Flags

```toml
[features]
default = ["simd"]
simd = []              # SIMD optimizations (default)
object-store = [...]   # S3, GCS, Azure (OmenDB only)
```

---

## oadb Requirements

### Must Have

| Requirement | seerdb Status | Notes |
|-------------|---------------|-------|
| Point lookups (get by ID) | ✅ | Fast path for vector metadata |
| Put/Delete operations | ✅ | Basic CRUD |
| WAL for durability | ✅ | Crash recovery |
| Configurable sync policy | ✅ | `SyncData`, `None` options |

### Nice to Have

| Requirement | seerdb Status | Notes |
|-------------|---------------|-------|
| Disable MVCC overhead | ❓ | Not currently configurable |
| Simpler compaction | ❓ | Could use single-level |
| Smaller memory footprint | ❓ | Default 256MB memtable |

### Recommended Config for oadb

```rust
DBOptions {
    memtable_capacity: 64 * 1024 * 1024,  // 64MB (vs default 256MB)
    wal_sync_policy: SyncPolicy::SyncData, // Or None for speed
    // No object-store feature
    ..Default::default()
}
```

---

## OmenDB Requirements

### Must Have (Current)

| Requirement | seerdb Status | Notes |
|-------------|---------------|-------|
| HNSW edge storage | ✅ | Via EdgeStorage wrapper |
| S3 tiering | ✅ | `object-store` feature |
| High write throughput | ✅ | 763K ops/sec |
| Compaction | ✅ | Reduce read amplification |

### Must Have (Future)

| Requirement | seerdb Status | Phase | Notes |
|-------------|---------------|-------|-------|
| **Full-text search (BM25)** | ❌ | Phase 10 | Inverted index needed |
| Inverted index | ❌ | Phase 10 | Term → doc ID mapping |
| Tokenization | ❌ | Phase 10 | Word splitting, stemming |

### Implementation Options for Full-Text Search

1. **Build in seerdb**
   - Pro: Single codebase, tight integration
   - Con: Significant new feature, complexity

2. **Separate module in OmenDB**
   - Pro: seerdb stays focused, faster iteration
   - Con: Two storage systems to manage

3. **Integrate tantivy**
   - Pro: Battle-tested Rust search engine
   - Con: External dependency, integration work

**Recommendation**: Start with option 2 (separate module), migrate to seerdb later if needed.

---

## Potential seerdb Improvements

### For oadb (Embedded Profile)

| Improvement | Impact | Effort |
|-------------|--------|--------|
| Configurable MVCC (disable) | Lower overhead | Low |
| Single-level compaction mode | Simpler, less I/O | Medium |
| Smaller default memtable | Lower memory | Low |

### For OmenDB (Cloud Profile)

| Improvement | Impact | Effort |
|-------------|--------|--------|
| Inverted index support | Full-text search | High |
| Better range scan perf | Analytics queries | Medium |
| Tiered storage policies | Cost optimization | Medium |

---

## Config Profiles (Proposed)

```rust
// Future: seerdb could have built-in profiles

impl DBOptions {
    /// Optimized for embedded/single-user workloads
    pub fn embedded() -> Self {
        Self {
            memtable_capacity: 64 * 1024 * 1024,  // 64MB
            wal_sync_policy: SyncPolicy::SyncData,
            // Disable background compaction threads?
            ..Default::default()
        }
    }

    /// Optimized for cloud/multi-tenant workloads
    pub fn cloud() -> Self {
        Self {
            memtable_capacity: 256 * 1024 * 1024,  // 256MB
            wal_sync_policy: SyncPolicy::SyncData,
            // Enable aggressive compaction
            ..Default::default()
        }
    }
}
```

---

## Test Status

| Suite | Status | Notes |
|-------|--------|-------|
| seerdb lib tests | ✅ 193 pass | Updated Nov 24, 2025 |
| seerdb integration | ✅ All pass | Crash recovery tests fixed |

---

## Action Items

### Immediate (No seerdb changes needed)

- [ ] Wire oadb to use seerdb with embedded-friendly config
- [ ] Verify seerdb works without `object-store` feature
- [ ] Benchmark seerdb for oadb workload (point lookups)

### Future (seerdb changes)

- [ ] Add `embedded()` config profile
- [ ] Consider disabling MVCC option
- [ ] Research inverted index integration for full-text search

---

## Contact

For seerdb changes that affect omendb/oadb:
1. Update this file with requirements
2. Discuss trade-offs before implementing
3. Test both products after changes

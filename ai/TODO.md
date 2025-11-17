# TODO - seerdb

**Last Updated**: November 17, 2025
**Current Sprint**: Performance Optimization & Cloud Integration
**Version**: 0.0.1-alpha (published to crates.io)
**Timeline**: Ongoing feature development

---

## ✅ COMPLETED: Block Cache Performance VALIDATED

**Status**: Block cache delivers **1,442x improvement** - FAR exceeds expectations! 🎉

**Results (omendb prefix scan benchmark)**:

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Cold cache scans | 22 QPS | **8,980 QPS** | **408x** |
| Hot cache scans | 22 QPS | **31,728 QPS** | **1,442x** |
| Random access | 22 QPS | **29,744 QPS** | **1,352x** |
| Cache hit rate | N/A | **97.38%** | Exceeds 80% target |

**What Was Done**:
- [x] Add `block_cache_capacity` to DBOptions (default: 16,384 blocks = 64MB)
- [x] Add `global_block_cache` field to DB struct
- [x] Wire global cache into SSTable operations
- [x] Tests for cache behavior (3 new tests, 162 total passing)
- [x] **Benchmark validated: 1,442x improvement for hot cache!**
- [x] Added omendb-specific benchmark: `examples/omendb_prefix_scan_benchmark.rs`

**Implementation Details**:
- Global shared cache across all SSTables
- Cache key: (path_hash, block_offset) for uniqueness
- LRU eviction via quick_cache (lock-free, concurrent-safe)
- Default 64MB cache (16,384 blocks × ~4KB average block size)
- 97.38% cache hit rate (exceeds 80% target)

---

## 📊 HIGH PRIORITY: Performance Profiling

**Goal**: Identify bottlenecks, optimize hot paths

- [ ] Profile with flamegraph (`cargo flamegraph`)
- [ ] Identify allocation hotspots (`dhat` or `heaptrack`)
- [ ] Measure cache hit rates (block cache, SSTable cache)
- [ ] Compare with RocksDB/fjall on real workloads:
  - [ ] omendb HNSW edge storage pattern
  - [ ] Time series writes (sequential timestamps)
  - [ ] Random key-value workload
- [ ] Analyze SIMD vs non-SIMD performance
- [ ] Profile lock contention (partitioned memtables)

**Specific omendb Targets**:
- [ ] Prefix scan latency (current: 45ms per scan)
- [ ] Graph edge storage pattern (key: node_id || level || neighbor_id)
- [ ] Hot node access patterns (frequently visited nodes)

---

## ☁️ MEDIUM PRIORITY: Complete Cloud Storage Integration

### Phase 1: Core Infrastructure ✅ COMPLETE
- [x] ObjectStoreBackend with S3, GCS, Azure support
- [x] Storage trait for pluggable backends
- [x] StorageConfig in DBOptions
- [x] Feature-gated: `--features object-store`
- [x] 6 unit tests passing

### Phase 2: Wire into DB (Next)
- [x] SSTableBuilder buffered writes ✅ **COMPLETE** (BufferedSSTableBuilder)
- [ ] Add Storage backend field to DB struct
- [ ] Use BufferedSSTableBuilder in flush path
- [ ] Use BufferedSSTableBuilder in compaction path
- [ ] Upload via ObjectStoreBackend
- [ ] Integration tests with cloud backend

**Note**: BufferedSSTableBuilder implemented - buffers in memory, single write/upload.

---

## 🚀 COMPLETED RECENTLY

### BufferedSSTableBuilder ✅ (November 17, 2025)
- In-memory SSTable builder (buffers all data)
- `finish_to_bytes()` for cloud uploads (S3/GCS/Azure)
- `finish_to_file()` for local disk (single write)
- Same API as SSTableBuilder
- 9 comprehensive tests added (168 total lib tests)
- **Enables**: Cloud storage uploads, fewer syscalls

### Global Block Cache ✅ (November 17, 2025)
- Shared cache across all SSTables (vs per-SSTable isolated caches)
- `block_cache_capacity` in DBOptions (default: 64MB)
- Cache key: (path_hash, block_offset) for uniqueness
- Metrics: block_cache_size, block_cache_capacity in DBStats
- 3 comprehensive tests added
- **Validated**: 1,442x improvement for omendb (22 QPS → 31,728 QPS)

### Object Store Infrastructure ✅
- ObjectStoreBackend (S3, GCS, Azure)
- StorageConfig enum
- Feature-gated compilation
- 6 unit tests passing

### Published to crates.io ✅
- Version: 0.0.1-alpha
- Crate name locked down: `seerdb`
- Tagged: `v0.0.1-alpha`
- 115 files, 210.7KiB compressed

### Snapshots ✅
- `db.snapshot()` - Point-in-time views
- `db.snapshot_consistent()` - Full consistency
- Range queries on snapshots
- 6 tests passing

### Convenience APIs ✅
- `db.iter()` - Full table iteration
- `db.prefix()` - Prefix scan
- Handles edge cases (0xFF overflow)
- 4 tests passing

### Bug Fixes ✅
- Bug #10: Merge iterator data loss (CRITICAL)
- Bug #11: Empty SSTable flush (CRITICAL)
- Batch atomicity, checksums, compaction safety

---

## 📅 Future Work (Lower Priority)

### API Enhancements
- [ ] `db.iter_rev()` - Reverse iteration
- [ ] `db.compact()` - Manual compaction trigger
- [ ] `ReadOptions`/`WriteOptions` - Per-operation configuration
- [ ] Column families/namespaces
- [ ] TTL/expiration

### Advanced Features
- [ ] MVCC transactions (multi-operation atomicity)
- [ ] VLog garbage collection
- [ ] Index optimization (bloom filter tuning)
- [ ] Compression level configuration

### Testing & Stability
- [ ] 72h+ soak tests
- [ ] Chaos testing (crash injection)
- [ ] Disk full scenarios
- [ ] Long-running fuzzing campaigns

---

## 📈 Success Metrics

### Performance Targets

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| Disk search | **31,728 QPS** | 200+ QPS | ✅ **1,442x better** |
| Cache hit rate | **97.38%** | >80% | ✅ **Exceeds target** |
| Write amp | 1.01x | <1.5x | ✅ Already excellent |
| Memory overhead | ~64MB | <100MB | ✅ Within budget |

### Quality Targets

| Metric | Current | Target |
|--------|---------|--------|
| Tests passing | 165 | All |
| Coverage | 81.54% | >80% |
| ASAN clean | ✅ | ✅ |
| Thread safety | 50+ tests | All passing |

---

## Next Session Plan

**Priority 1**: Wire Object Store into DB ✅ **READY TO START**
- BufferedSSTableBuilder is complete (prerequisite done!)
- Add Storage backend to DB struct
- Use BufferedSSTableBuilder in flush/compaction paths
- Upload via ObjectStoreBackend (S3/GCS/Azure)
- 0.5 days estimated

**Priority 2**: Performance profiling
- Flamegraph analysis
- Identify other bottlenecks (block cache is not the issue now!)
- Ongoing work

**Priority 3**: Test with actual cloud backends
- Integration tests with S3 (localstack or real)
- Verify upload/download cycle
- Performance comparison

---

**Tests**: 174 passing (168 lib + 6 object-store)
**Coverage**: 81.54%
**Performance**: 2.47x RocksDB writes, 2.07x reads, **1,442x prefix scans (with cache)**
**Version**: 0.0.1-alpha (published)

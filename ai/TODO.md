# TODO - seerdb

**Last Updated**: November 17, 2025
**Current Sprint**: Performance Optimization & Cloud Integration
**Version**: 0.0.1-alpha (published to crates.io)
**Timeline**: Ongoing feature development

---

## 🔥 HIGHEST PRIORITY: Block Cache (omendb performance)

**Impact**: 10-20x improvement for disk search (22 QPS → 200+ QPS)

**Problem**: seerdb caches SSTable metadata only, not data blocks. Every prefix scan hits disk.

**Implementation** (see `ai/BLOCK_CACHE_OPTIMIZATION.md`):
- [ ] Add `block_cache_capacity` to DBOptions (default: 64MB)
- [ ] Add `block_cache` field to DB struct (Arc<Cache>)
- [ ] Add `id` field to SSTable for cache key
- [ ] Modify SSTable::read_block() to check cache first
- [ ] Add cache hit/miss metrics to DBStats
- [ ] Tests for cache behavior
- [ ] Benchmark before/after

**Expected Outcome**:
- Cache hit rate: >80% for hot workloads
- Disk search: 22 QPS → 200+ QPS
- Memory overhead: <10% over cache size

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
- [ ] SSTableBuilder buffered writes (accumulate in memory)
- [ ] Add Storage backend field to DB struct
- [ ] Use Storage trait in flush path
- [ ] Use Storage trait in compaction path
- [ ] Integration tests with cloud backend

**Note**: Buffered writes may improve local performance too (fewer syscalls).

---

## 🚀 COMPLETED RECENTLY

### Published to crates.io ✅
- Version: 0.0.1-alpha
- Crate name locked down: `seerdb`
- Tagged: `v0.0.1-alpha`
- 115 files, 210.7KiB compressed

### Object Store Infrastructure ✅
- ObjectStoreBackend (S3, GCS, Azure)
- StorageConfig enum
- Feature-gated compilation
- 6 unit tests passing

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

| Metric | Current | Target | Priority |
|--------|---------|--------|----------|
| Disk search | 22 QPS | 200+ QPS | HIGH |
| Cache hit rate | N/A | >80% | HIGH |
| Write amp | 1.01x | <1.5x | MEDIUM |
| Memory overhead | ~40MB | <100MB | LOW |

### Quality Targets

| Metric | Current | Target |
|--------|---------|--------|
| Tests passing | 162 | All |
| Coverage | 81.54% | >80% |
| ASAN clean | ✅ | ✅ |
| Thread safety | 50+ tests | All passing |

---

## Next Session Plan

**Priority 1**: Block cache implementation
- Highest ROI for omendb
- 1-2 days estimated
- See `ai/BLOCK_CACHE_OPTIMIZATION.md`

**Priority 2**: SSTableBuilder buffering
- Enables cloud storage
- May improve local perf
- 1-2 days estimated

**Priority 3**: Performance profiling
- Flamegraph analysis
- Identify other bottlenecks
- Ongoing work

---

**Tests**: 162 passing (156 lib + 6 object-store)
**Coverage**: 81.54%
**Performance**: 2.47x RocksDB writes, 2.07x reads
**Version**: 0.0.1-alpha (published)

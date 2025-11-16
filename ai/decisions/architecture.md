# Architecture Decisions

**Format**: Decision → Rationale → Trade-offs → References

---

## 1. Base Structure: LSM Tree (Not B+ Tree)

**Decision**: Use LSM-tree as foundation (like RocksDB), not B+ tree (like sled)

**Rationale**:
- LSM trees optimize for write-heavy workloads
- All target workloads (database vectors, queue, time series) are write-heavy
- Research papers focus on LSM optimizations (learned components fit naturally)

**Trade-offs**:
- ✅ Better write amplification
- ✅ More research to build on
- ❌ More complex than B+ tree
- ❌ Read performance requires optimization (bloom filters, caching)

**References**: Dostoevsky paper (LSM trade-offs), WiscKey (KV separation), PebblesDB (fragmented LSM)

---

## 3. Key-Value Separation (WiscKey-Style)

**Decision**: Store large values (>4KB threshold) separately in value log (vLog)

**Rationale**:
- database vectors are 512-4096 bytes (embeddings)
- WiscKey shows 10-100x write amplification reduction
- LSM size reduction: 50x smaller (100GB → 2GB for 1KB values)
- Industrial validation: BlobDB, Titan, TerarkDB, BadgerDB
- Compaction only rewrites keys + pointers (not full values)

**Implementation**:
- **vLog**: Append-only log for values >4KB threshold
- **LSM tree**: Stores keys + value references (offset + length)
- **GC**: Head-tail tracking, validity check via LSM, rewrite valid entries
- **Range queries**: Parallel prefetching to mitigate random I/O

**Threshold Tuning**:
- BadgerDB: 4KB default
- TerarkDB: 512B default
- seerdb initial: 4KB (tune based on database workload benchmarks)
- Rule: Separate if value_size > key_size * 10 (rough heuristic)

**Trade-offs**:
- ✅ Write amplification: 10-100x reduction
- ✅ LSM size: 50x smaller (more in cache, faster compaction)
- ✅ Database loading: 2.5-111x faster
- ✅ Lookups: 1.6-14x faster
- ❌ Space amplification: Higher (garbage until GC)
- ❌ Crash recovery: Slower (vLog head pointer overhead)
- ❌ Range scans: Random I/O (mitigated by prefetching)
- ❌ Small values: Overhead not worth it

**When to Use**:
- ✅ Values >1KB that dominate storage (database vectors: YES)
- ✅ Write-heavy workloads (omen: append-heavy documents)
- ✅ SSDs (can exploit parallel reads)
- ❌ Small uniform values <256B (queue metadata: NO)
- ❌ Range-scan dominated without prefetch capability

**References**:
- "WiscKey: Separating Keys from Values" (Lu et al., FAST 2016)
- Industrial implementations: Titan, TerarkDB, BadgerDB, BlobDB

---

## 4. Rust-Native Implementation

**Decision**: Build in Rust (not C++ like RocksDB)

**Rationale**:
- Memory safety without GC overhead
- Easier integration with database (also Rust)
- Modern async/await for I/O
- SIMD intrinsics well-supported

**Trade-offs**:
- ✅ Memory safety
- ✅ Easy database integration
- ✅ Modern tooling
- ❌ Less mature ecosystem than C++ (fewer libraries)
- ❌ Learning curve for unsafe code (if needed)

---

## 5. Apache 2.0 (Source-Available)

**Decision**: Use Apache 2.0 (not MIT/Apache)

**Rationale**:
- Prevents cloud providers from offering managed seerdb
- Allows self-hosting and embedding
- Source-available (not closed source)
- Same license as database ecosystem

**Trade-offs**:
- ✅ Protects commercial interests
- ✅ Still allows most use cases
- ❌ Not OSI-approved "open source"
- ❌ May limit adoption vs permissive license

# DECISIONS - seerdb Design Decisions

**Format**: Decision → Rationale → Trade-offs → References

---

## Architecture Decisions

### 1. Base Structure: LSM Tree (Not B+ Tree)

**Decision**: Use LSM-tree as foundation (like RocksDB), not B+ tree (like sled)

**Rationale**:
- LSM trees optimize for write-heavy workloads
- All target workloads (omen vectors, queue, time series) are write-heavy
- Research papers focus on LSM optimizations (learned components fit naturally)

**Trade-offs**:
- ✅ Better write amplification
- ✅ More research to build on
- ❌ More complex than B+ tree
- ❌ Read performance requires optimization (bloom filters, caching)

**References**: Dostoevsky paper (LSM trade-offs), WiscKey (KV separation), PebblesDB (fragmented LSM)

---

### 2. Learned Bloom Filters (Week 1 Priority)

**Decision**: Replace traditional bloom filters with learned models

**Rationale**:
- 90% space reduction claim (Kraska et al., 2018)
- Low implementation complexity (good first prototype)
- Immediate benefit (every SSTable uses bloom filters)

**Trade-offs**:
- ✅ 90% space savings
- ✅ Same false positive rate
- ❌ Training cost on compaction
- ❌ Model inference latency (vs hash function)

**Validation**: Prototype in Week 1 to verify 90% claim

**References**: "Learned Bloom Filters" (Kraska et al., 2018)

---

### 3. Key-Value Separation (WiscKey-Style)

**Decision**: Store large values (>4KB threshold) separately in value log (vLog)

**Rationale**:
- omen vectors are 512-4096 bytes (embeddings)
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
- seerdb initial: 4KB (tune based on omen workload benchmarks)
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
- ✅ Values >1KB that dominate storage (omen vectors: YES)
- ✅ Write-heavy workloads (omen: append-heavy documents)
- ✅ SSDs (can exploit parallel reads)
- ❌ Small uniform values <256B (queue metadata: NO)
- ❌ Range-scan dominated without prefetch capability

**References**:
- "WiscKey: Separating Keys from Values" (Lu et al., FAST 2016)
- Industrial implementations: Titan, TerarkDB, BadgerDB, BlobDB

---

### 4. Rust-Native Implementation

**Decision**: Build in Rust (not C++ like RocksDB)

**Rationale**:
- Memory safety without GC overhead
- Easier integration with omen (also Rust)
- Modern async/await for I/O
- SIMD intrinsics well-supported

**Trade-offs**:
- ✅ Memory safety
- ✅ Easy omen integration
- ✅ Modern tooling
- ❌ Less mature ecosystem than C++ (fewer libraries)
- ❌ Learning curve for unsafe code (if needed)

---

### 5. Elastic License 2.0 (Source-Available)

**Decision**: Use Elastic License 2.0 (not MIT/Apache)

**Rationale**:
- Prevents cloud providers from offering managed seerdb
- Allows self-hosting and embedding
- Source-available (not closed source)
- Same license as omen ecosystem

**Trade-offs**:
- ✅ Protects commercial interests
- ✅ Still allows most use cases
- ❌ Not OSI-approved "open source"
- ❌ May limit adoption vs permissive license

---

## Research Phase Decisions

### 6. 4-Week Research Phase (Before Coding)

**Decision**: Spend 4 weeks reading papers and benchmarking before building core engine

**Rationale**:
- Avoid reimplementing RocksDB (need to understand research landscape)
- Design decisions require deep understanding (can't undo later)
- Validate research claims early (prototype learned bloom filters)

**Trade-offs**:
- ✅ Informed design decisions
- ✅ Avoid costly rewrites
- ❌ Delays functional product
- ❌ Risk of "analysis paralysis"

**Mitigation**: Prototype learned bloom filters in Week 1 (stay grounded in implementation)

---

### 7. Workload-Aware Optimization (Tucana-Inspired)

**Decision**: Detect workload patterns and adapt compaction strategy

**Rationale**:
- omen has distinct workloads (append-heavy vectors, FIFO queue, time series)
- Generic LSM tuning (RocksDB) suboptimal for all
- Tucana shows 3x throughput improvement vs RocksDB

**Trade-offs**:
- ✅ Better performance for known workloads
- ✅ Unique differentiator vs RocksDB
- ❌ Complexity (workload detection, strategy switching)
- ❌ May perform worse on unknown workloads

**Implementation**: Week 16 (after core engine stable)

**References**: "Tucana" (Liu et al., 2020)

---

## Future Decisions (TBD)

### 8. Learned Index Model Selection (DECIDED - Use ALEX)

**Decision**: Use ALEX-style learned index for SSTable index blocks

**Options Evaluated**:
- ✅ ALEX (gapped arrays, handles updates, proven)
- Piecewise linear models (Bourbon - for immutable data only)
- Neural networks (original Kraska paper - too complex)

**Rationale**:
- ALEX code available in omen-org/ (can adapt)
- Handles updates/deletes (critical for dynamic data)
- 2.2x faster than original learned index, 4.1x faster than B+trees
- Production-quality implementation exists (Microsoft Research)

**Status**: Decided, implementation in Phase 2 (Weeks 9-12)

---

### 9. Compaction Strategy (DECIDED - Lazy Leveling)

**Decision**: Use Lazy Leveling (Dostoevsky) for seerdb

**Options Evaluated**:

1. **Leveled (RocksDB default)**:
   - Write amp: High (11x at T=10)
   - Read amp: Low (disjoint key ranges)
   - Use case: Read-heavy workloads
   - ❌ Too much write amp for omen workload

2. **Tiered (Cassandra-style)**:
   - Write amp: Low (good for writes)
   - Read amp: High (must check all runs)
   - Space amp: Very high (O(T), 1.2GB → 9.3GB at T=4)
   - Use case: Pure write-heavy workloads
   - ❌ Space amp too high, read performance poor

3. **Lazy Leveling (Dostoevsky)** ✅ **CHOSEN**:
   - Largest level: Leveled (disjoint for range queries)
   - Other levels: Tiered (reduce write amp)
   - Write amp: Better than leveled
   - Read amp: Better than tiered
   - Space amp: Similar to leveled (~11%)
   - Use case: Mixed workloads (omen vectors)

4. **Fragmented (PebblesDB)**:
   - Write amp: Best (2.4-3x better than RocksDB)
   - Read amp: Worst (multiple sstables per guard)
   - Use case: Pure write-heavy, no range scans
   - ❌ omen needs range queries (vector search top-K)

**Rationale**:
- **omen vectors**: Append-heavy + range scans (vector search top-K)
- Lazy Leveling balances both needs perfectly
- Largest level disjoint → efficient range queries
- Upper levels tiered → reduced write amplification
- Can combine with WiscKey (KV separation for large embeddings)

**Configuration**:
- Level ratio: T=10 (RocksDB standard, tune later)
- Largest level: Leveled compaction (merge all overlaps)
- Levels 0 to N-1: Tiered compaction (allow multiple runs)
- Adaptive tuning: Future enhancement (Phase 3)

**Workload Mapping**:
- **omen vectors**: Lazy Leveling ✅ (balanced read/write, range scans)
- **omen-queue**: Tiered (pure write-heavy, FIFO, no range scans)
- **omen time series**: Lazy Leveling (append-heavy + time-range queries)

**Trade-offs**:
- ✅ Best balance for mixed workloads
- ✅ omen workload fits perfectly
- ✅ Can add adaptive tuning later (Dostoevsky model)
- ❌ More complex than pure leveled or tiered
- ❌ Need to implement both strategies

**References**:
- "Dostoevsky: Better Space-Time Trade-Offs" (Dayan & Idreos, SIGMOD 2018)
- "PebblesDB" (Raju et al., SOSP 2017) - considered but rejected for omen

**Status**: Decided, implementation in Phase 1 (Week 7)

---

### 10. I/O Backend (DECIDED - tokio with optional io_uring)

**Decision**: Use tokio async I/O by default, io_uring as opt-in feature

**Options Evaluated**:
- ✅ tokio async I/O (default, cross-platform, secure)
- io_uring (Linux 5.1+, optional, security concerns)
- Hybrid approach chosen

**Rationale - Security First**:
- **io_uring vulnerabilities**: 77 CVEs, 60% of 2022 kernel exploits
- Critical issues: CVE-2021-20226, CVE-2022-2602, CVE-2023-2598 (all privilege escalation to root)
- Common attack vectors: use-after-free, reference counting bugs, out-of-bounds access
- Complex kernel/userspace shared memory attack surface

**Implementation Strategy**:
- **Default**: tokio async I/O (safe, cross-platform, good performance)
- **Optional**: io_uring feature flag (Linux-only, opt-in, document risks)
- **Disabled by default**: Users must explicitly enable with awareness of security trade-offs

**Configuration**:
```toml
[features]
default = ["tokio-io"]
io-uring = ["io-uring-sys"]  # Opt-in, Linux-only, performance vs security trade-off
```

**Performance Trade-off**:
- tokio: Excellent performance, ~10-20% slower than io_uring in best case
- io_uring: 50-100% faster I/O (when it works), but serious security risks
- **Decision**: Security > marginal performance gains

**Trade-offs**:
- ✅ Secure by default (no privilege escalation risk)
- ✅ Cross-platform (macOS, Linux, Windows)
- ✅ Simpler implementation (standard async/await)
- ❌ Slightly slower than io_uring (acceptable trade-off)
- ❌ Linux users don't get max I/O performance by default

**Status**: Decided, tokio default, io_uring opt-in behind feature flag

---

*Add decisions as they're made - include commit hash if implemented*

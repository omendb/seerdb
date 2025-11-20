# Workload Optimization Strategies

**Purpose**: Define target workloads and the specific architectural optimizations `seerdb` employs for each.

---

## 1. Large Value Workloads
**Characteristics**:
- Large values (blobs/documents: 512-4096 bytes)
- Append-heavy (new entries)
- Range scans (sequential access)
- Hot/cold data (recent entries hot)

**seerdb Optimizations**:
- **Key-Value Separation (WiscKey)**: Large values stored in separate vLog to avoid rewriting them during compaction. Minimizes write amplification.
- **Learned Index (ALEX)**: Predicts ID patterns for faster lookups in SSTables.
- **LZ4 Compression**: Highly effective on large values.
- **Workload-Aware Compaction**: Detects append-heavy patterns to reduce unnecessary merging.

## 2. Time Series Workloads
**Characteristics**:
- Sorted by timestamp
- Range queries (time windows)
- Compression-friendly (similar values)
- Long retention (old data archived)

**seerdb Optimizations**:
- **Time-Aware Compaction**: Merges SSTables by time ranges to keep recent data hot and effectively archive old data.
- **Aggressive Compression**: Delta encoding + LZ4.
- **Hot/Cold Separation**: Recent data stays in lower levels/cache; old data migrates to cheap storage (future S3 integration).
- **Efficient Range Scans**: Prefix iterators optimized for sequential access.

## 3. Graph Workloads (Vector Indexes)
**Characteristics**:
- **Frequent Prefix Scans**: Edge traversal in HNSW graphs (key format: `NodeID | Level | NeighborID`).
- **High Concurrency**: Massive read/write parallelism.
- **Small Values**: Edges are small (<128 bytes).

**seerdb Optimizations**:
- **Partitioned Memtables**: 16 shards to reduce lock contention during concurrent writes.
- **Lock-Free WAL**: Minimizes latency for high-throughput ingestion.
- **High-Performance Prefix Iterators**: Custom iterator path avoiding full key comparisons.
- **Block Cache**: Optimized for scan locality (caching uncompressed blocks).

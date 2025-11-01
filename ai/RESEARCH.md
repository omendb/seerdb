# RESEARCH - seerdb Research Notes

**Purpose**: Quick research notes, explorations, and findings that don't fit elsewhere

---

## Learned Data Structures - Core Concepts

### Key Insight (Kraska et al., 2018)
Data structures are models:
- B-tree: model of key distribution (CDF approximation)
- Hash table: uniform distribution model
- Bloom filter: set membership model

If data has patterns → ML models can replace traditional structures
- Result: 10-100x space savings, similar or better speed
- Trade-off: Training cost, model complexity

---

## Research Questions

### Week 1 Questions (Updated After Reading Papers)

**Q: Can learned bloom filters really achieve 90% space reduction?**
- Status: **Claims vary widely (17-97%)** - need prototype to validate
- Finding: Original 90% claim may be optimistic, even 50% is valuable
- Approach: Start with simple model (decision tree), measure on real data
- Architecture: Basic LBF (model + backup filter) first, then sandwiched variant

**Q: How expensive is model training during compaction?**
- Status: **Low risk for SSTables** (immutable after creation)
- Finding: Train once during compaction, no retraining needed
- Models: Decision trees or GBT (fast training, fast inference)
- Comparison needed: Training time vs space savings

**Q: What happens when workload changes (learned models become stale)?**
- Status: **ALEX solves this for indexes**
- Finding: Gapped arrays + periodic model retraining
- For bloom filters: Not a problem (SSTable blooms are immutable)
- For indexes: ALEX-style adaptive retraining (models adapt to new inserts)

**Q: Which learned component to implement first?**
- Answer: **Learned bloom filters** (simpler than learned indexes)
- Rationale: No update handling needed (immutable), easier to validate
- ALEX already in omen (can use existing implementation later)

---

## Interesting Papers (Not on Core List)

### Found While Reading
- "The Case for Learned Index Structures" references:
  - "Learned Cardinality Estimation" (similar approach for query optimization)
  - "Neural Data Structures" (broader ML + data structures)

### Discovered During Research
- **Partitioned Learned Bloom Filter** (ICLR 2021)
  - Multiple thresholds + separate backup filters per region
  - 4-6 partitions optimal
  - Better space/FPR trade-off than basic LBF

- **Stable Learned Bloom Filters for Data Streams** (VLDB 2020)
  - Adapts to changing data distributions
  - 97% space savings claim (on streaming data)
  - Relevant for omen-queue (high throughput, dynamic workload)

---

## Benchmarking Notes

### Tools to Install
- RocksDB (baseline)
- sled (Rust B+ tree)
- fjall (Rust LSM, modern)
- PebblesDB (fragmented LSM, if available)

### Metrics to Measure
- Throughput (ops/sec): writes, reads, scans
- Latency (p50, p95, p99)
- Write amplification (bytes written / bytes in DB)
- Space amplification (bytes on disk / bytes in DB)

### YCSB Workloads
- Workload A: 50% reads, 50% updates (update heavy)
- Workload B: 95% reads, 5% updates (read mostly)
- Workload C: 100% reads (read only)
- Workload D: 95% reads, 5% inserts (read latest)
- Workload E: 95% scans, 5% inserts (short ranges)
- Workload F: 100% read-modify-write (RMW)

---

## Implementation Notes

### Learned Bloom Filter Architecture (From Research)

**Basic LBF**:
```
Query(key) → Model.predict(key) → score > threshold?
                                   ↓ Yes: Return "in set"
                                   ↓ No: Check backup_bloom_filter(key)
```

**Sandwiched LBF** (Mitzenmacher optimization):
```
Query(key) → initial_bloom(key) → false?
                                  ↓ Yes: Return "not in set"
                                  ↓ No: Model.predict(key) → score > threshold?
                                        ↓ Yes: Return "in set"
                                        ↓ No: Check backup_bloom_filter(key)
```

**Training Process**:
1. Collect positive examples: All keys in SSTable
2. Generate negative examples: Random keys NOT in SSTable (or Zipf distribution)
3. Train binary classifier (decision tree or GBT)
4. Choose threshold to balance FPR and backup filter size
5. Build backup bloom filter for keys with score < threshold
6. Serialize: model + backup filter → SSTable metadata

**ML Libraries for Rust**:
- **linfa**: Rust-native ML (decision trees, logistic regression)
- **smartcore**: More algorithms, including GBT
- **tract**: Neural network inference (if we want NNs later)

---

## Random Notes

### omen Vector Workload Characteristics
- Large values: 512-4096 bytes (embeddings)
- Append-heavy: new documents added, rarely updated
- Read patterns: vector search returns top-K (range scan)
- Hot data: recent documents queried more

**seerdb Optimization Ideas** (Validated by Papers):
- ✅ KV separation (WiscKey) - large embeddings → separate log (10x write amp reduction)
- ✅ Learned index (ALEX) - document IDs likely sequential (2-4x speedup, 2000x smaller)
- ✅ Learned bloom (LBF) - 50-90% space savings
- Tiered compaction (optimize for appends) - need to read Dostoevsky paper

### ALEX Code Available
- ALEX implementation archived in omen-org/ repository
- Can adapt for seerdb SSTable indexes when needed
- Focus prototype effort on learned bloom filters first (simpler, validate concept)

---

*Add notes as you research - organize later if needed*

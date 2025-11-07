# State-of-the-Art Research: Learned Indexes + LSM Trees (2024-2025)

**Last Updated**: November 6, 2025
**Focus**: Latest research on learned indexes in LSM-tree systems

---

## Executive Summary

**Key Finding**: Learned indexes in LSM systems is an active research area (5+ major papers in 2024-2025)

**Main Themes**:
1. **Learned indexes IN LSM-trees** - Replacing bloom filters and block indexes
2. **Workload-aware tuning** - Using ML to optimize LSM parameters
3. **Data sortedness exploitation** - Learned indexes perform better on sorted data
4. **Read-write tradeoffs** - Learned components help reads, may hurt writes

**Relevance to seerdb**:
- ✅ We're on the right track (ALEX indexes, learned blooms)
- ⚠️ Missing: Workload-aware tuning, adaptive ML models
- 🎯 Opportunity: Implement CAMAL-style active learning

---

## 1. "Evaluating Learned Indexes in LSM-tree Systems" (June 2025)

**Authors**: Junfeng Liu, Jiarui Ye, Mengshi Chen (NTU Singapore)
**Link**: https://arxiv.org/abs/2506.08671
**Status**: arXiv preprint (very recent!)

### Key Contributions

**Problem**: Previous learned index studies focused on simple baseline models. Unclear if **recent advances** in learned indexes improve LSM performance.

**Study**: Comprehensive evaluation of advanced learned indexes in LSM systems
- Multiple learned index types (ALEX, PGM, RMI, etc.)
- Different LSM workloads (read-heavy, write-heavy, mixed)
- Comparison with traditional bloom filters and block indexes

### Key Findings

1. **Learned indexes help reads significantly**
   - 20-50% improvement on point queries
   - 30-70% improvement on range queries
   - Benefit increases with larger datasets

2. **Write performance impact**
   - Model retraining overhead: 5-15% write slowdown
   - Trade-off: Better reads vs slower writes
   - Static workloads benefit most

3. **Memory vs accuracy trade-off**
   - Smaller models: Less memory, lower accuracy
   - Larger models: More memory, better accuracy
   - Sweet spot: 2-4KB per SSTable for 1M keys

4. **Data characteristics matter**
   - **Sorted data**: 50-100x space savings vs bloom filters
   - **Random data**: 10-20x space savings
   - **Skewed distributions**: Learned indexes excel

### Relevance to seerdb

✅ **We're doing**: ALEX indexes in SSTables
⚠️ **We're missing**:
- Adaptive model selection based on data characteristics
- Model retraining strategy (we retrain on every compaction)
- Memory budget optimization

🎯 **Action items**:
1. Benchmark learned bloom filter space savings (claim: 90% reduction)
2. Measure model retraining overhead during compaction
3. Implement adaptive model complexity based on SSTable size

---

## 2. "CAMAL: Optimizing LSM-trees via Active Learning" (Sept 2024)

**Authors**: Weiping Yu, Siqiang Luo, Zihao Yu, Gao Cong
**Link**: https://arxiv.org/abs/2409.15130
**Status**: arXiv preprint

### Key Contributions

**Problem**: LSM-tree tuning is hard - many parameters (level ratio, bloom filter bits, compaction strategy) with complex interactions.

**Solution**: Use **active learning** to tune LSM-tree configuration
- ML model predicts cost of read/write operations
- Coupled with traditional cost models
- Adapts to workload changes

### How CAMAL Works

1. **Cost Model**: Predict read/write costs for given LSM config
2. **Active Learning**: Sample configurations intelligently (not random)
3. **Online Tuning**: Adjust parameters as workload changes
4. **Safety**: Bounded by traditional cost models (prevents bad configs)

### Results

- **30-50% throughput improvement** over default RocksDB config
- **10-20% improvement** over hand-tuned configurations
- Works across different workloads (read-heavy, write-heavy, mixed)

### Relevance to seerdb

❌ **We don't have**: Workload-aware parameter tuning
🎯 **Opportunity**:
- Implement workload detection (key distribution, access patterns)
- Auto-tune: compaction strategy, bloom filter size, memtable size
- Use ML to predict optimal vLog threshold per workload

**Implementation Path**:
1. **Phase 1**: Collect workload metrics (key distribution, read/write ratio)
2. **Phase 2**: Implement rule-based tuning (if write-heavy, use tiered compaction)
3. **Phase 3**: ML-based tuning (train model on workload → config mapping)

---

## 3. "Benchmarking Learned and LSM Indexes for Data Sortedness" (2024)

**Authors**: Aneesh Raman, Andy Huynh, Jinqi Lu, Manos Athanassoulis (Boston University)
**Link**: https://cs-people.bu.edu/mathan/publications/dbtest24-raman.pdf

### Key Contributions

**Problem**: Real data is often **partially sorted** (timestamps, IDs, etc.). Do learned indexes and LSM-trees exploit this?

**Study**: First-ever study on behavior of learned indexes and LSM-trees with varying data sortedness

### Key Findings

1. **Learned indexes exploit sortedness much better than LSM-trees**
   - 5-10x faster ingestion on 80% sorted data
   - LSM-trees: Similar performance regardless of sortedness
   - B+-trees: Cannot exploit sortedness during ingestion

2. **LSM compaction overhead**
   - Compaction costs dominate write path
   - Sortedness doesn't help reduce compaction
   - Learned indexes reduce lookup cost during compaction

3. **Recommendations**:
   - Use learned indexes for **sorted/semi-sorted data**
   - LSM-trees still best for **random data**
   - Hybrid approach: learned index on sorted levels, traditional on random levels

### Relevance to seerdb

✅ **We handle**: Vector DBs (timestamps), queue (FIFO) - naturally sorted!
🎯 **Opportunity**:
- Detect sortedness during ingestion
- Optimize ALEX index for sorted data (simpler models)
- Skip compaction for fully sorted levels (WORM optimization)

**Implementation**:
1. Measure sortedness during memtable flush (% of keys in order)
2. If >90% sorted: Use linear model (fast, simple)
3. If <50% sorted: Use ALEX (adaptive, robust)

---

## 4. "Bf-Tree: Modern Read-Write-Optimized Concurrent Larger-Than-Memory Range Index" (Aug 2024)

**Authors**: Xiangpeng Hao, Badrish Chandramouli (UW-Madison, Microsoft Research)
**Published**: VLDB 2024
**Link**: https://badrishc.github.io/papers/bftree-vldb2024.pdf

### Key Contributions

**Problem**: B-Trees have inefficient caching (entire pages) and high write amplification (whole page updates)

**Solution**: Bf-Tree - separates **caching** from **storage organization**
- Cache: Individual hot records (not whole pages)
- Storage: Page-organized for efficient disk I/O
- Result: 5-10x less cache memory, 50-70% lower write amp

### Architecture

1. **Hot record cache**: Only cache frequently accessed records
2. **Cold page storage**: Infrequently accessed data in pages
3. **FASTER-style log**: Hybrid log for updates

### Results

- **5-10x less memory** for same cache hit rate
- **50-70% lower write amp** than B-tree
- **2-3x higher throughput** on YCSB workloads

### Relevance to seerdb

⚠️ **Different architecture**: We're LSM, they're B-tree
✅ **Shared insight**: Separate caching from storage
🎯 **Idea**: Apply to our block cache
- Currently: Cache entire blocks (4KB-16KB)
- Bf-Tree approach: Cache individual hot records
- Benefit: More effective use of block cache memory

**Potential Optimization**:
- Track access frequency per key (not per block)
- Cache hot keys individually (bypass block cache)
- Keep cold keys in block cache (batch reads)

---

## 5. "A LSM-Tree Combined with Read Hotness and Learned Index" (Oct 2025)

**Authors**: IEEE publication
**Link**: https://ieeexplore.ieee.org/document/10825283/
**Status**: Very recent!

### Key Contributions (Limited info from abstract)

**Approach**: Combine read hotness tracking with learned indexes in LSM-tree

**Main Idea**:
- Track which keys are frequently read (read hotness)
- Optimize learned index for hot keys (better accuracy)
- Use simpler models for cold keys (save memory)

### Potential Relevance

🎯 **Idea for seerdb**:
- Track read frequency in block cache
- Adjust ALEX index: More complex for hot keys, simpler for cold
- Benefit: Better memory efficiency

---

## Additional Research Threads

### 1. SIMD in LSM Trees

**Finding**: Modern LSM engines use SIMD for:
- Key comparisons in merge operations
- Bloom filter lookups (multiple hash functions in parallel)
- Compression/decompression

**Status in seerdb**: We explored SIMD, deemed premature
**Revisit when**: After range scan optimization complete

### 2. io_uring for Async I/O

**Linux io_uring** (May 2019): Zero-copy, zero-syscall async I/O
- 50-100% faster than traditional async I/O
- Used in modern storage engines (VelarixDB claims to use it)

**Status in seerdb**: Not implemented
**Opportunity**:
- Use io_uring for SSTable reads during compaction
- Batch multiple SSTable reads in single syscall
- Potential: 2x faster compaction

### 3. Adaptive Readahead

**RocksDB optimization**: Predict sequential access, prefetch blocks

**How it works**:
- Detect sequential range scans
- Prefetch next blocks in background
- Reduce read latency by 30-50%

**Status in seerdb**: We load blocks on-demand (lazy)
**Opportunity**: Implement readahead for range scans

---

## Research Gaps We Can Fill

### 1. Learned Indexes + Key-Value Separation

**Observation**: No published research on combining:
- Learned indexes (reduce lookup cost)
- WiscKey-style KV separation (reduce write amp)

**We're doing both!** Potential for research paper.

### 2. Workload-Aware Learned Models

**Idea**: Adjust learned model complexity based on:
- Workload type (read-heavy → complex models, write-heavy → simple)
- Data characteristics (sorted → linear, random → ALEX)
- Hardware (SSD → optimize for sequential, NVMe → random OK)

**Status**: No comprehensive study exists
**We could**: First to combine all three dimensions

### 3. Rust-Specific Optimizations

**Observation**: Most research uses C++/Java implementations

**Opportunity**: Show how Rust features improve LSM:
- Zero-cost abstractions (iterators)
- Type safety (fewer bugs)
- Memory safety (no leaks during compaction)
- Async/await (easier io_uring integration)

---

## Action Items for seerdb

### Immediate (Phase 7 - Range Scan Fix)
1. ✅ Implement SSTable filtering (skip non-overlapping ranges)
2. 🎯 Benchmark learned bloom filter space savings
3. 🎯 Measure ALEX index impact on read performance

### Near-term (Phase 8 - Research Validation)
1. 🎯 Validate write amp claims (1.01x) vs fjall
2. 🎯 Measure model retraining overhead during compaction
3. 🎯 Implement data sortedness detection
4. 🎯 Adaptive model selection (sorted → linear, random → ALEX)

### Medium-term (Phase 9 - SOTA Integration)
1. 🎯 Workload detection (CAMAL-inspired)
2. 🎯 Auto-tuning (compaction strategy, bloom size, vLog threshold)
3. 🎯 Read hotness tracking (optimize learned index for hot keys)
4. 🎯 io_uring integration for async I/O

### Research Opportunities
1. 📄 **Paper**: "Learned Indexes + KV Separation in LSM Trees"
2. 📄 **Blog**: "Why Rust Makes Better Storage Engines"
3. 📄 **Benchmark**: seerdb vs fjall vs RocksDB (comprehensive)

---

## Key Takeaways

1. ✅ **We're on the right track** - Learned indexes in LSM is cutting-edge
2. ⚠️ **We're behind on tuning** - Need workload-aware optimization
3. 🎯 **Low-hanging fruit**:
   - Data sortedness detection
   - Adaptive model selection
   - io_uring integration
4. 📄 **Publication opportunity** - Unique combination of features

---

## References

1. Liu et al., "Evaluating Learned Indexes in LSM-tree Systems", arXiv:2506.08671, June 2025
2. Yu et al., "CAMAL: Optimizing LSM-trees via Active Learning", arXiv:2409.15130, Sept 2024
3. Raman et al., "Benchmarking Learned and LSM Indexes for Data Sortedness", DBTEST 2024
4. Hao & Chandramouli, "Bf-Tree: Modern Read-Write-Optimized Concurrent Range Index", VLDB 2024
5. IEEE, "A LSM-Tree Combined with Read Hotness and Learned Index", Oct 2025
6. Kraska et al., "The Case for Learned Index Structures", SIGMOD 2018 (foundational)
7. Lu et al., "WiscKey: Separating Keys from Values in SSD-conscious Storage", FAST 2016

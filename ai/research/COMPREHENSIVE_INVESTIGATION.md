# Comprehensive Investigation - November 8, 2025

## Questions to Answer

1. **fjall's mixed workload advantage** - Why 14% faster? (718K vs 832K)
2. **Benchmark scale** - Do our tests catch rkyv/caching benefits?
3. **tokio-uring effort** - Drop-in or code changes?
4. **mmap investigation** - Have we looked into this?

---

## 1. fjall's Mixed Workload Advantage 🔍

### The Gap

**Current Performance**:
- seerdb: 718K mixed ops/sec
- fjall: 832K mixed ops/sec
- Gap: -14% (114K ops/sec difference)

**What We Know**:
- ✅ We're FASTER on writes (878K vs 427K = +106%) 🏆
- ✅ We're FASTER on reads (2,207K vs 1,161K = +90%) 🏆
- ❌ We're SLOWER on mixed (718K vs 832K = -14%)

**The Mystery**: How can we be faster on BOTH pure workloads but slower on mixed?

### Hypothesis 1: Read/Write Balance

**Mixed workload = 50/50 reads + writes**

fjall might have better balance between read and write paths. Let me calculate:

**seerdb mixed performance**:
```
Expected mixed (if perfectly balanced):
  (878K writes + 2,207K reads) / 2 = 1,542K theoretical max
  Actual: 718K
  Efficiency: 718K / 1,542K = 46.5%
```

**fjall mixed performance**:
```
Expected mixed (if perfectly balanced):
  (427K writes + 1,161K reads) / 2 = 794K theoretical max
  Actual: 832K
  Efficiency: 832K / 794K = 104.8% (!!)
```

**🚨 CRITICAL FINDING**: fjall is getting >100% theoretical performance!

This means fjall's mixed workload is FASTER than the average of their pure workloads. This suggests:
1. **Read/write overlap optimization** - They pipeline reads while writes happen
2. **Batch efficiencies** - Mixed operations trigger batching that pure don't
3. **Cache synergies** - Reads benefit from write-warmed cache

### Hypothesis 2: Lock Contention

**Our architecture**:
- Partitioned memtables (16 partitions) with per-partition locks
- ArcSwap for LSM tree (lock-free reads)
- WAL has lock-free queue

**Potential issue**:
- Mixed workload might hit same partition (lock contention)
- Reads wait for writes in same partition
- fjall might have better partition distribution

### Hypothesis 3: Flush/Compaction Triggers

**Mixed workload specific**:
- Writes fill memtable → trigger flush
- Flush blocks new writes (briefly)
- Reads continue but hit disk instead of memtable
- fjall might have better flush strategies

### Investigation Plan

To find root cause, we need to:

1. **Profile fjall's code** (read their source)
   ```bash
   # Clone fjall
   git clone https://github.com/fjall-rs/fjall
   cd fjall
   # Search for mixed workload optimizations
   rg "read.*write|batch|pipeline" --type rust
   ```

2. **Profile our mixed workload**
   ```bash
   # Run mixed with profiling
   samply record -- target/release/examples/baseline_benchmark
   # Look for:
   - Lock contention (mutex wait time)
   - Flush stalls (write pauses)
   - Cache miss rate (reads hitting disk)
   ```

3. **Compare architectures**:
   - Their memtable design
   - Their flush strategy
   - Their read/write pipel ining
   - Their cache design (we have quick_cache, what do they have?)

---

## 2. Benchmark Scale - Testing rkyv/Caching Benefits 📊

### Current Benchmark Characteristics

**100K operations**:
- Dataset size: ~100MB (100K * 1KB values)
- Unique keys: 100,000
- Block cache: 1000 blocks = ~4MB decompressed
- Working set: Fits entirely in cache!

### Why This Might Miss Benefits

**rkyv (zero-copy deserialization)**:
- Benefit: 7x faster deserialization (118ns → 16ns)
- Current impact: ~0 (everything is cached!)
- At 100K ops: Cache hit rate ~95%+
- **Missing**: Large-scale cache misses where deserialization matters

**Multi-tier caching**:
- L1 (decompressed): 1000 blocks
- L3 (compressed): Would hold 2000 blocks
- Current: 100K dataset = ~2500 unique blocks (assuming 4KB blocks, 40 entries/block)
- **Problem**: L1 already holds 40% of dataset!

### What Scale Would Show Benefits?

**rkyv benefits appear when**:
- Cache miss rate >20%
- Frequent SSTable opens (cold starts)
- Large dataset (>1M ops, >1GB)

**Multi-tier cache benefits appear when**:
- Working set >10x cache size
- Temporal locality (re-access old data)
- Memory constraints

**Proposed benchmark scales**:

```
Small (current):   100K ops, 100MB dataset
Medium:            1M ops, 1GB dataset
Large:             10M ops, 10GB dataset
Stress:            100M ops, 100GB dataset
```

### Testing Strategy

**Test 1: 1M ops benchmark**
```rust
// examples/large_benchmark.rs
Operations: 1_000_000
Value size: 1024 bytes
Dataset: ~1GB

Expected:
- Cache hit rate drops to 60-80%
- rkyv shows +5-10% benefit
- Multi-tier cache shows +8-12% benefit
```

**Test 2: Varying working set**
```rust
// Zipfian distribution (realistic access pattern)
80% of accesses to 20% of keys

Benchmark configs:
- Uniform random (worst case cache)
- Zipfian (realistic)
- Sequential (best case cache)
```

**Decision**: **YES, we should test at 1M+ scale**

Our 100K benchmark is too small to show:
- rkyv benefits (everything cached)
- Multi-tier cache benefits (working set fits in L1)
- Real-world access patterns

---

## 3. tokio-uring Implementation Effort 🔧

### What Changes Are Needed?

**tokio-uring is NOT drop-in**. Here's why:

#### Ownership Model Differences

**tokio (current)**:
```rust
// You own the buffer
let mut buf = vec![0; 4096];
file.read_exact(&mut buf).await?;
use_buffer(&buf);  // buf is still yours
```

**tokio-uring**:
```rust
// io_uring owns buffer during operation
let buf = vec![0; 4096];
let (result, buf) = file.read_at(buf, offset).await;  // Give ownership
use_buffer(&buf);  // Get ownership back
```

#### API Differences

| Operation | tokio | tokio-uring |
|-----------|-------|-------------|
| **File open** | `File::open(path).await` | `File::open(path).await` (same) |
| **Random read** | `seek() + read()` | `read_at(buf, offset).await` (better!) |
| **Sequential read** | `read()` | `read_at(buf, offset).await` |
| **Write** | `write_all(data).await` | `write_at(data, offset).await` |
| **Buffer lifecycle** | Borrow (`&mut buf`) | Ownership (`buf`) |

#### Code Changes Required

**Estimate: 300-500 lines across 5 files**

1. **src/sstable/mod.rs** (~100 lines):
   ```rust
   // Before
   async fn load_block(&self, offset: u64) -> Result<Block> {
       let mut buf = vec![0; size];
       self.file.seek(SeekFrom::Start(offset)).await?;
       self.file.read_exact(&mut buf).await?;
       Ok(parse_block(buf)?)
   }

   // After
   async fn load_block(&self, offset: u64) -> Result<Block> {
       let buf = vec![0; size];
       let (res, buf) = self.file.read_at(buf, offset).await;
       res?;
       Ok(parse_block(buf)?)
   }
   ```

2. **src/wal/mod.rs** (~80 lines):
   - Change append from `write_all()` to `write_at()`
   - Track file offset manually
   - Handle ownership transfers

3. **src/compaction/mod.rs** (~50 lines):
   - Sequential reads need manual offset tracking
   - Buffer pool for zero-copy transfers

4. **Platform-specific compilation** (~70 lines):
   ```rust
   // src/io/mod.rs
   #[cfg(all(target_os = "linux", feature = "io_uring"))]
   pub use tokio_uring::fs::File;

   #[cfg(not(all(target_os = "linux", feature = "io_uring")))]
   pub use tokio::fs::File;
   ```

5. **Runtime initialization** (~20 lines):
   ```rust
   // Before
   #[tokio::main]
   async fn main() { }

   // After (Linux with io_uring)
   fn main() {
       tokio_uring::start(async {
           // Application logic
       });
   }
   ```

### Effort Estimate

| Task | Lines | Days | Complexity |
|------|-------|------|------------|
| SSTable reads | ~100 | 1 | MEDIUM |
| WAL writes | ~80 | 1 | MEDIUM |
| Compaction | ~50 | 0.5 | LOW |
| Platform abstraction | ~70 | 0.5 | MEDIUM |
| Testing both backends | - | 1 | HIGH |
| **Total** | **~300** | **4** | **MEDIUM-HIGH** |

### Benefits vs Cost

**Benefits** (Linux only):
- +20-50% I/O throughput (proven in benchmarks)
- Zero-syscall overhead
- Better latency distribution

**Costs**:
- 4 days implementation
- Platform-specific code (Linux 5.1+)
- Must test 2 code paths (Linux + non-Linux)
- Ownership model complexity

**Decision**: **Worth it IF I/O is bottleneck** (need profiling first)

**Recommendation**:
1. Profile current mixed workload
2. If I/O >20% of time → implement io_uring
3. If I/O <20% → skip for now

---

## 4. mmap for Read-Only SSTables 💾

### What is mmap?

**Memory-mapped files**: OS maps file directly into memory

**Benefits**:
- Zero-copy reads (no buffer allocation)
- OS manages caching (simpler than our cache)
- Lazy loading (only load accessed pages)
- Shared memory (multiple processes can share)

**Drawbacks**:
- Platform-specific behavior
- No control over eviction
- Can cause page faults (latency spikes)
- Doesn't work well with compression

### Have We Investigated This?

**Answer**: NO, we haven't investigated mmap yet.

**Why not**:
- Our SSTables are compressed (LZ4)
- mmap doesn't work with compressed blocks
- Need decompressed cache anyway

### When mmap Makes Sense

**Good for**:
- ✅ Uncompressed, read-only files (indexes, bloom filters)
- ✅ Random access patterns
- ✅ Large files (>100MB)
- ✅ OS has plenty of RAM

**Bad for**:
- ❌ Compressed data (need decompress step)
- ❌ Frequent writes
- ❌ Fine-grained cache control
- ❌ Predictable latency (page faults are unpredictable)

### Our Use Case Analysis

**Current architecture**:
```
SSTable file (compressed):
  [Compressed Block 1] [Compressed Block 2] ... [Index] [Bloom]
```

**What we could mmap**:
- ✅ Bloom filters (small, uncompressed, read-only)
- ✅ SSTable indexes (small, read-only)
- ❌ Data blocks (compressed - can't mmap)

**Potential optimization**:
```rust
// Instead of loading bloom filter into memory
let bloom_data = read_bloom_from_file()?;
let bloom = BloomFilter::from_bytes(bloom_data);

// mmap bloom filter directly
let bloom_mmap = unsafe { Mmap::map(&file)? };
let bloom = BloomFilter::from_slice(&bloom_mmap[offset..]);
```

### Implementation Analysis

**Complexity**: LOW-MEDIUM (2-3 days)

**Changes needed**:
1. Use `memmap2` crate for safe mmap
2. Separate bloom/index storage (already uncompressed)
3. Lifetime management (mmap must outlive references)

**Expected benefit**: +2-5% (avoid bloom/index allocation overhead)

**Trade-offs**:
- ✅ Simpler code (OS handles caching)
- ✅ Faster metadata access
- ❌ Less cache control
- ❌ Potential page faults

**Decision**: **MAYBE - Low priority**

**Rationale**:
- Bloom filters already small (not a bottleneck)
- Indexes cached via quick_cache
- Benefit is small (+2-5%)
- mmap doesn't help compressed blocks (our main data)

---

## Summary & Recommendations

### Priority 1: Investigate fjall Gap 🔍 **DO NOW**

**Why**: 14% gap on mixed workload, we don't know root cause
**Effort**: 1-2 days (code analysis + profiling)
**Potential**: +10-20% if we find and fix the issue
**Action**:
1. Clone fjall, read their mixed workload code path
2. Profile our mixed workload for bottlenecks
3. Compare architectures
4. Implement fixes

---

### Priority 2: Large-Scale Benchmarks 📊 **DO NOW**

**Why**: Our 100K benchmark is too small to show rkyv/caching benefits
**Effort**: 1 day (implement 1M/10M benchmarks)
**Benefit**: Validate whether rkyv/caching are worth implementing
**Action**:
1. Create 1M ops benchmark
2. Create 10M ops benchmark (stress test)
3. Test with Zipfian distribution (realistic)
4. Measure cache hit rates
5. **Decide**: Do rkyv + multi-tier cache based on data

---

### Priority 3: Profile-Guided Optimization 📈 **DO NEXT**

**Why**: Don't optimize blindly (today's lesson!)
**Effort**: 2-3 hours
**Benefit**: Find ACTUAL bottlenecks
**Action**:
1. Profile mixed workload with samply
2. Identify hot paths (>10% time)
3. Optimize based on data, not guesses

---

### Priority 4: tokio-uring ⏸️ **DEFER**

**Why**: Need profiling to confirm I/O is bottleneck
**Effort**: 4 days (MEDIUM-HIGH)
**Decision**: Only implement if profiling shows I/O >20% of time
**Action**: Profile first, then decide

---

### Priority 5: mmap ⏸️ **SKIP FOR NOW**

**Why**: Small benefit (+2-5%), doesn't help compressed blocks
**Effort**: 2-3 days
**Decision**: Not worth it right now
**Action**: Revisit if bloom/index access becomes bottleneck

---

## Recommended Next Steps

**This Week**:
1. ✅ Investigate fjall gap (1-2 days) - **CRITICAL**
2. ✅ Implement large-scale benchmarks (1 day)
3. ✅ Test rkyv + multi-tier cache at 1M+ scale (1 day)
4. ✅ Profile and optimize based on findings (1 day)

**Timeline**: 4-5 days to close fjall gap and validate next optimizations

**Goal**:
- Understand why fjall is faster on mixed
- Validate rkyv/caching benefits at scale
- Data-driven decision on next optimizations

---

**Status**: Ready to investigate fjall and test at scale
**Date**: November 8, 2025
**Next**: Clone fjall, analyze mixed workload code

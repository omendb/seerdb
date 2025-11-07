# SOTA Algorithmic Improvements

**Date**: November 7, 2025
**Focus**: Research-backed algorithmic optimizations, not parameter tweaking

---

## Current State: What's Already Implemented

✅ **WiscKey (2016)**: Key-value separation with vLog (write amp: 1.01x)
✅ **Learned Bloom Filters (2018)**: ML-based set membership testing
✅ **K-way Merge (SOTA)**: BinaryHeap-based range scans
✅ **ALEX Learned Index**: Used in omen for efficient lookups

**Gap**: Still 2x slower than fjall on writes (218K vs 423K ops/sec)

---

## Priority 1: Dostoevsky LSM Tuning ⭐⭐⭐

**Paper**: "Dostoevsky: Better Space-Time Trade-Offs" (Dayan et al., Harvard 2018)

### Current Implementation (Suboptimal)

```rust
// src/db.rs - Generic LSM parameters
pub struct DBOptions {
    base_level_size: 10 * 1024 * 1024,  // 10MB
    size_ratio: 10,                      // Generic 10x ratio
    num_levels: 7,                       // Fixed 7 levels
}
```

**Problem**: One-size-fits-all parameters. Dostoevsky paper shows optimal ratios depend on workload.

### Dostoevsky Algorithm

**Key insight**: Trade off between write amplification and read amplification based on workload.

**Lazy Leveling** (for write-heavy):
- L0: Multiple overlapping runs (no merge)
- L1-L6: Single run per level (traditional leveling)
- **Result**: 50% less write amp, slightly more read amp

**Tiering** (for read-heavy):
- All levels: Multiple overlapping runs
- Merge only when full
- **Result**: 10x less write amp, 10x more read amp

**Implementation**:

```rust
#[derive(Clone, Copy)]
enum CompactionStrategy {
    Leveling,       // Traditional (balanced)
    LazyLeveling,   // Write-optimized (Dostoevsky)
    Tiering,        // Extreme write optimization
}

impl DBOptions {
    // Auto-select based on workload
    pub fn optimize_for_workload(&mut self, read_write_ratio: f64) {
        if read_write_ratio < 0.3 {
            // Write-heavy: Use lazy leveling
            self.strategy = LazyLeveling;
            self.size_ratio = 4;  // Smaller ratio for write-heavy
        } else if read_write_ratio > 3.0 {
            // Read-heavy: Use traditional leveling
            self.strategy = Leveling;
            self.size_ratio = 10;  // Larger ratio for read-heavy
        } else {
            // Balanced: Use lazy leveling with moderate ratio
            self.strategy = LazyLeveling;
            self.size_ratio = 7;
        }
    }
}
```

**Expected gain**: +20-30% writes (reduce write amp from 1.01x to 0.7x)
**Complexity**: Medium (3-5 days)
**Research**: Proven by Dostoevsky paper

---

## Priority 2: Prefix Compression (SOTA) ⭐⭐⭐

**Papers**: Multiple (PebblesDB, LevelDB, RocksDB)

### Current Implementation (No Compression)

```rust
// SSTable stores full keys:
// "user:123:name" -> "Alice"
// "user:123:email" -> "alice@example.com"
// "user:123:age" -> "25"
```

**Waste**: "user:123:" repeated 3 times = 30 bytes of redundancy

### Prefix Compression Algorithm

**Technique**: Store prefix once, only suffixes in subsequent entries

```rust
// Block format with prefix compression:
// Restart point 0:
//   "user:123:name" -> "Alice" (full key)
//   "age" -> "25" (shared prefix: "user:123:")
//   "email" -> "alice@example.com" (shared prefix: "user:123:")
// Restart point 1:
//   "user:456:name" -> "Bob" (new full key)
```

**Implementation** in `BlockBuilder`:

```rust
struct BlockBuilder {
    prefix_len: usize,  // Length of shared prefix
    last_key: Bytes,    // Previous key for prefix calculation
}

impl BlockBuilder {
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> bool {
        // Calculate shared prefix length
        let prefix_len = if !self.last_key.is_empty() {
            key.iter()
                .zip(self.last_key.iter())
                .take_while(|(a, b)| a == b)
                .count()
        } else {
            0
        };

        // Encode: [prefix_len][suffix_len][suffix][value_len][value]
        let suffix_len = key.len() - prefix_len;
        let encoded_size = 2 + suffix_len + 4 + value.len();

        if self.buffer.len() + encoded_size > self.capacity {
            return false;
        }

        // Write compressed entry
        self.buffer.extend_from_slice(&(prefix_len as u16).to_le_bytes());
        self.buffer.extend_from_slice(&(suffix_len as u16).to_le_bytes());
        self.buffer.extend_from_slice(&key[prefix_len..]);
        self.buffer.extend_from_slice(&(value.len() as u32).to_le_bytes());
        self.buffer.extend_from_slice(value);

        self.last_key = Bytes::copy_from_slice(key);
        true
    }
}
```

**Expected gain**:
- 30-50% space reduction for typical workloads
- Faster I/O (less data to write/read)
- +15-25% write throughput (less data to encode/decode)

**Complexity**: Medium (3-4 days)
**Research**: Standard technique in all modern LSMs

---

## Priority 3: Partitioned Memtables ⭐⭐

**Papers**: Tucana (2020), FASTER (2018)

### Current Implementation (Single Skiplist)

```rust
struct DB {
    memtable: Arc<Mutex<Memtable>>,  // One large skiplist
}
```

**Problem**:
- Lock contention on high concurrency
- Poor cache locality (large data structure)
- O(log n) insert where n = all keys

### Partitioned Approach

**Technique**: Multiple smaller memtables, hash-partitioned

```rust
const NUM_PARTITIONS: usize = 16;  // CPU core count

struct DB {
    memtables: [Arc<Mutex<Memtable>>; NUM_PARTITIONS],
}

impl DB {
    fn partition_for_key(&self, key: &[u8]) -> usize {
        let hash = xxhash(key);
        hash % NUM_PARTITIONS
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let partition = self.partition_for_key(key);
        let mt = &self.memtables[partition];

        // Lock only one partition (less contention)
        mt.lock().unwrap().put(key, value);
        Ok(())
    }
}
```

**Benefits**:
- 16x less lock contention (16 partitions)
- Better cache locality (smaller working set)
- Faster inserts: O(log(n/16)) instead of O(log n)

**Expected gain**: +25-40% writes on multi-core systems
**Complexity**: High (5-7 days, affects all code paths)
**Research**: Proven by FASTER, Tucana papers

---

## Priority 4: SIMD Key Comparisons ⭐⭐

**Papers**: Multiple (general SIMD optimization literature)

### Current Implementation (Scalar)

```rust
// Skiplist key comparison (hot path)
fn compare_keys(a: &[u8], b: &[u8]) -> Ordering {
    a.cmp(b)  // Byte-by-byte comparison
}
```

**Problem**: Scalar comparison checks 1 byte at a time

### SIMD Approach

**Technique**: Compare 16-32 bytes at once using SIMD instructions

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

unsafe fn compare_keys_simd(a: &[u8], b: &[u8]) -> Ordering {
    let len = a.len().min(b.len());
    let mut i = 0;

    // Process 16 bytes at a time with SSE2
    while i + 16 <= len {
        let a_vec = _mm_loadu_si128(a[i..].as_ptr() as *const __m128i);
        let b_vec = _mm_loadu_si128(b[i..].as_ptr() as *const __m128i);

        let cmp = _mm_cmpeq_epi8(a_vec, b_vec);
        let mask = _mm_movemask_epi8(cmp);

        if mask != 0xFFFF {
            // Found difference - find first different byte
            let diff_pos = mask.trailing_ones();
            return a[i + diff_pos as usize].cmp(&b[i + diff_pos as usize]);
        }

        i += 16;
    }

    // Handle remaining bytes
    a[i..].cmp(&b[i..])
}
```

**Expected gain**: +5-15% overall (key comparison is ~10-20% of time)
**Complexity**: Medium (2-3 days, platform-specific)
**Research**: Standard optimization in high-performance systems

---

## Priority 5: Bloom Filter SIMD ⭐

**Current**: Scalar hash computation

### SIMD Bloom Filter Operations

```rust
unsafe fn bloom_check_simd(bloom: &[u64], hashes: &[u64; 4]) -> bool {
    // Check 4 hash positions simultaneously
    let mut bits = _mm_setzero_si128();

    for &hash in hashes {
        let idx = hash as usize % bloom.len();
        let bit = _mm_set1_epi64x(bloom[idx] as i64);
        bits = _mm_or_si128(bits, bit);
    }

    // Check if all required bits are set
    let mask = _mm_movemask_epi8(_mm_cmpeq_epi8(bits, _mm_set1_epi8(-1)));
    mask == 0xFFFF
}
```

**Expected gain**: +3-5% (bloom filter checks are frequent)
**Complexity**: Low (1-2 days)

---

## Priority 6: Lock-Free Skiplist ⭐

**Papers**: "A Practical Lock-Free Skiplist" (Fraser, 2004)

### Current Implementation

```rust
// crossbeam-skiplist is already lock-free at the data structure level
// But Arc<Mutex<Memtable>> adds lock overhead
```

### True Lock-Free Approach

Use atomics for memtable swap (no mutex):

```rust
struct DB {
    memtable: AtomicPtr<Memtable>,
}

impl DB {
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        loop {
            let mt = self.memtable.load(Ordering::Acquire);
            unsafe { (*mt).put(key, value) }?;

            if !unsafe { (*mt).should_flush() } {
                break;
            }

            // Try to swap for flush
            self.try_swap_for_flush(mt)?;
        }
        Ok(())
    }
}
```

**Expected gain**: +10-20% (eliminate mutex overhead)
**Complexity**: High (unsafe, careful memory management)

---

## What's NOT Worth Doing (Parameter Tweaking)

❌ **Disable vLog by default**: Not algorithmic, just hiding a feature
❌ **Change memtable size**: Parameter tuning, not optimization
❌ **Adjust batch sizes**: Already optimized
❌ **Change level ratios** (without Dostoevsky math): Random tuning

---

## Recommended Implementation Order

### Phase 1: Space/IO Optimizations (1-2 weeks)

1. **Prefix compression** (+15-25% writes, less I/O)
2. **SIMD key comparisons** (+5-15% overall)

**Expected**: 218K → 290K writes (+33%)

### Phase 2: Concurrency Optimizations (2-3 weeks)

3. **Partitioned memtables** (+25-40% writes)
4. **Lock-free memtable access** (+10-20%)

**Expected**: 290K → 410K writes (+41% from phase 1)

### Phase 3: LSM Tuning (1-2 weeks)

5. **Dostoevsky lazy leveling** (+20-30% from better write amp)

**Expected**: 410K → 500K writes (+22% from phase 2)

**Total**: 218K → 500K writes (+129%, exceeds fjall's 423K)

---

## Research Papers to Implement

1. ✅ **WiscKey (2016)**: Already implemented (vLog)
2. ⏳ **Dostoevsky (2018)**: Lazy leveling, optimal ratios
3. ⏳ **PebblesDB (2017)**: Fragmented LSM, prefix compression
4. ⏳ **Tucana (2020)**: Partitioned memtables, workload-aware
5. ⏳ **FASTER (2018)**: Lock-free concurrent access

---

## Validation

Each optimization must:
1. ✅ Have research paper backing
2. ✅ Show measurable improvement (>10%)
3. ✅ Pass all 126 tests
4. ✅ Benchmark vs baseline

**No parameter tweaking without algorithmic justification.**

---

**Status**: Ready to implement
**Timeline**: 4-6 weeks for all optimizations
**Expected result**: 218K → 500K writes (2.3x improvement, beat fjall)

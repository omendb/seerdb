# Hot Path Analysis - Storage I/O Optimization

**Date**: November 9, 2025
**Context**: After file handle reuse optimization (3-11x speedup), identify remaining hot paths

---

## Methodology

Analyzed code paths in SSTable read operations to identify CPU-intensive operations that could benefit from optimization.

---

## Hot Paths Identified

### 1. ✅ File I/O (OPTIMIZED)

**Status**: Optimized with file handle reuse pattern
**Before**: 130.8µs (LocalStorage with open/close per read)
**After**: 11.0µs (Arc<Mutex<File>> reuse)
**Speedup**: 11.9x faster

**No further optimization needed**

---

### 2. Block::new() - Block Parsing on Cache Miss

**Called**: Every time a block is loaded from disk (cache misses only)
**Frequency**: 5-10% of reads (90%+ cache hit rate target)

**Operations**:
```rust
pub fn new(data: Bytes) -> Result<Self> {
    // 1. CRC32C checksum verification (hardware accelerated)
    let computed_checksum = crc32c::crc32c(&data[..data.len() - 4]);  // ✅ Optimized

    // 2. LZ4 decompression (if compressed)
    let uncompressed_data = decompress_size_prepended(compressed_slice)?;  // ✅ Optimized

    // 3. Parse restart points (varint decoding)
    loop {
        let _offset = cursor.read_u64_varint()?;  // Small overhead
        num_restarts += 1;
    }

    Ok(Self {
        data: Bytes::from(uncompressed_data),
        restart_offset,
        num_restarts,
        decompressed_cache: Arc::new(OnceLock::new()),  // Lazy init
    })
}
```

**Analysis**:
- CRC32C: Hardware accelerated (SSE4.2/ARM CRC instructions) ✅
- LZ4: Fast decompression library (lz4_flex) ✅
- Varint parsing: Minimal overhead (~1-2µs for typical block)

**Optimization Potential**: Low (10-15% at most)

---

### 3. decompress_all_entries() - Entry Parsing (Lazy)

**Called**: First time block is accessed (iter() or find_exact())
**Frequency**: Once per block, cached thereafter

**Operations**:
```rust
fn decompress_all_entries(&self) -> Vec<(Bytes, Bytes)> {
    let mut entries = Vec::new();
    let mut last_key = Bytes::new();

    loop {
        // 1. Read prefix_len, suffix_len (varint)
        let prefix_len = cursor.read_u64_varint()?;
        let suffix_len = cursor.read_u64_varint()?;

        // 2. Read suffix (slice - zero copy)
        let suffix = self.data.slice(offset..offset + suffix_len);

        // 3. Reconstruct key (allocation + memcpy)
        let key = if prefix_len == 0 {
            suffix.clone()  // Zero copy
        } else {
            // Allocate BytesMut + copy prefix + copy suffix
            let mut key_data = BytesMut::with_capacity(prefix_len + suffix_len);
            key_data.extend_from_slice(&last_key[..prefix_len]);  // ← ALLOCATION
            key_data.extend_from_slice(&suffix);                   // ← MEMCPY
            key_data.freeze()
        };

        // 4. Read value_len (varint) + value (slice - zero copy)
        let value_len = cursor.read_u64_varint()?;
        let value = self.data.slice(offset..offset + value_len);  // Zero copy

        last_key = key.clone();
        entries.push((key, value));
    }

    entries
}
```

**Hot Spots**:
1. **Varint decoding**: 3× per entry (prefix_len, suffix_len, value_len)
2. **BytesMut allocation**: For keys with prefix compression (~94% of keys at RESTART_INTERVAL=16)
3. **Memcpy**: Copying prefix + suffix to reconstruct keys
4. **Bytes cloning**: last_key clone for next iteration

**Optimization Potential**: Medium (20-30% possible)

**Challenges**:
- Prefix compression format not compatible with zero-copy deserialization
- Must reconstruct keys from prefix + suffix
- Varint decoding requires parsing

---

### 4. Block Cache Lookups (quick_cache)

**Status**: Already optimized with lock-free LRU
**Library**: quick_cache (designed for high-performance Rust workloads)

**No optimization needed**

---

## Optimization Opportunities

### Option 1: rkyv Zero-Copy Deserialization

**Potential Benefit**: Eliminate varint decoding + allocation overhead
**Challenge**: Prefix compression incompatible with zero-copy

**Analysis**:
- rkyv works best with fixed-size structs that can be memory-mapped
- Prefix compression requires dynamic reconstruction: `key = prefix[0..N] + suffix`
- Can't use rkyv without changing block format

**Two approaches**:

#### A. Replace Prefix Compression with rkyv Format
```rust
#[derive(Archive, Serialize, Deserialize)]
struct BlockEntry {
    key: Vec<u8>,    // Full key (no prefix compression)
    value: Vec<u8>,  // Full value
}
```

**Pros**:
- Zero-copy deserialization (no parsing)
- Faster decompression (~20-30% improvement)

**Cons**:
- Larger block sizes (no prefix compression) → worse compression ratio
- More disk I/O (blocks are bigger)
- Trade-off: save CPU, spend more I/O

#### B. Hybrid: rkyv for Values, Keep Prefix Compression for Keys
```rust
#[derive(Archive, Serialize, Deserialize)]
struct BlockValue {
    data: Vec<u8>,
}

// Keys still use prefix compression (small, benefit from compression)
// Values use rkyv (large, benefit from zero-copy)
```

**Pros**:
- Zero-copy for values (which are larger: embeddings, documents)
- Keep prefix compression for keys (small, high compression ratio)

**Cons**:
- Complexity: two serialization formats
- Marginal benefit: values are already zero-copy via Bytes::slice()

---

### Option 2: Optimize Varint Decoding (SIMD)

**Current**: Byte-by-byte parsing
**SIMD**: Vectorized varint decoding (see: `varint-simd` crate)

**Potential Speedup**: 2-3x varint decoding (from ~1-2µs to ~0.5µs per block)

**Complexity**: Low (drop-in replacement)

**Recommendation**: ⏳ Evaluate if profiling shows varint is bottleneck

---

### Option 3: Pre-Allocate Entry Vec

**Current**: `Vec::new()` + push (reallocates as grows)
**Optimized**: `Vec::with_capacity(estimated_entries)`

```rust
fn decompress_all_entries(&self) -> Vec<(Bytes, Bytes)> {
    // Estimate entries based on average entry size (4KB block / 100 bytes = ~40 entries)
    let estimated_entries = self.data.len() / 100;
    let mut entries = Vec::with_capacity(estimated_entries);
    // ...
}
```

**Potential Speedup**: 5-10% (fewer reallocations)
**Complexity**: Trivial
**Recommendation**: ✅ Easy win, do it

---

### Option 4: Optimize Prefix Reconstruction

**Current**: BytesMut allocation + extend_from_slice (2 memcpy ops)
**Optimized**: Single allocation + copy

```rust
// Current (2 allocations + 2 memcpy)
let mut key_data = BytesMut::with_capacity(prefix_len + suffix_len);
key_data.extend_from_slice(&last_key[..prefix_len]);
key_data.extend_from_slice(&suffix);

// Optimized (1 allocation + 1 memcpy via unsafe)
let mut key_data = Vec::with_capacity(prefix_len + suffix_len);
unsafe {
    std::ptr::copy_nonoverlapping(
        last_key[..prefix_len].as_ptr(),
        key_data.as_mut_ptr(),
        prefix_len,
    );
    std::ptr::copy_nonoverlapping(
        suffix.as_ptr(),
        key_data.as_mut_ptr().add(prefix_len),
        suffix_len,
    );
    key_data.set_len(prefix_len + suffix_len);
}
let key = Bytes::from(key_data);
```

**Potential Speedup**: 10-15% (faster memcpy)
**Complexity**: Medium (unsafe code)
**Recommendation**: ⏳ Only if profiling shows this is a bottleneck

---

## Decision Matrix

| Optimization | Speedup | Complexity | Compatibility | Recommendation |
|--------------|---------|------------|---------------|----------------|
| **File handle reuse** | **11.9x** ✅ | Low | ✅ Done | **✅ SHIPPED** |
| Pre-allocate Vec | 5-10% | Trivial | ✅ Compatible | **✅ DO IT** |
| SIMD varint | 2-3x varint | Low | ✅ Compatible | ⏳ Profile first |
| Optimize memcpy | 10-15% | Medium (unsafe) | ✅ Compatible | ⏳ Profile first |
| rkyv (no prefix) | 20-30% CPU | High | ❌ Breaks format | ❌ Not worth it |
| rkyv (hybrid) | ~5% | High | ⚠️ Complex | ❌ Not worth it |

---

## Recommendations

### Immediate (0.0.1)
1. ✅ **File handle reuse** - COMPLETE (11.9x speedup)
2. ✅ **Pre-allocate entry Vec** - Easy 5-10% win, trivial change

### Deferred (0.0.2+)
3. ⏳ **Profile with production workloads** - Get real data on varint/memcpy overhead
4. ⏳ **SIMD varint** - If profiling shows it's a bottleneck
5. ⏳ **Optimize memcpy** - If profiling shows it's a bottleneck

### Not Recommended
- ❌ **rkyv** - Incompatible with prefix compression, marginal benefits don't justify format change

---

## rkyv Evaluation Summary

**Question**: Should we use rkyv for zero-copy deserialization?

**Answer**: ❌ **No, not worth it for current format**

**Reasoning**:
1. **Prefix compression incompatible**: rkyv requires fixed-layout structs, can't reconstruct keys dynamically
2. **Values already zero-copy**: Using Bytes::slice() for values (no allocation)
3. **Format change required**: Would need to drop prefix compression → larger blocks → more I/O
4. **Trade-off unfavorable**: Save ~20-30% CPU, spend ~30-40% more I/O (net loss)

**Alternative**: If we ever redesign the block format for object storage (Phase 2), consider rkyv at that time with full keys (no prefix compression). For local disk, prefix compression + LZ4 is the right choice.

---

**Next Steps**: Implement Vec::with_capacity() pre-allocation (trivial 5-10% win), then move to production testing.

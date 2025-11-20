# PLAN V2 - Core Engine Optimization

**Goal**: Transition `seerdb` from a functional prototype to a high-performance, memory-efficient storage engine suitable for `omendb` (Vector/Graph workloads).

**Status**: DRAFT (Nov 19, 2025)

## 1. Buffer Management (LeanStore Architecture)

The current `BufferPool` implementation (Phase 1) is functional but inefficient (allocates memory on every load, double buffers with `block_cache`).

### Phase 2: Memory Efficiency & Reuse
*   **Problem**: `BufferPool::get_page` replaces the `Vec<u8>` in the frame with a new one from the loader. This defeats the purpose of a buffer pool (reuse).
*   **Solution**: 
    *   Modify `get_page` to provide `&mut [u8]` (or `&mut Vec<u8>`) to the loader.
    *   Loader reads directly into the existing Frame memory.
    *   Handle variable sized blocks:
        *   If `size <= frame_size`: Use existing buffer.
        *   If `size > frame_size`: Reallocate (grow) the buffer.
*   **Status**: ✅ Completed

### Phase 3: Zero-Copy Access
*   **Problem**: `load_block` copies data from `Frame` to `Bytes` (for `Block`).
*   **Solution**: 
    *   Make `Block` capable of holding a `FrameRef` directly.
    *   `Block` becomes a view into the `BufferPool`.
    *   Eliminates `block_cache` (or makes it a lightweight "Swip" cache).
    *   See `ai/design/PHASE_3_ZERO_COPY.md` for detailed design.
*   **Status**: ✅ Completed (Benchmark: 30% faster for uncompressed blocks)

### Phase 4: Pointer Swizzling (True LeanStore)
*   **Concept**: Replace `PageId` lookups with direct pointers (`&Frame`) in the index.
*   **Implementation**:
    *   `TopLevelIndexEntry` stores `Atomic<Swip>` (Swizzled Pointer).
    *   `Swip` = `PageId` (Disk) OR `Arc<Frame>` (Memory).
    *   Requires unsafe Rust or careful `Arc` management.
*   **Status**: 🔮 Future

## 2. Compaction & Storage Format

### Cloud-Native SSTables
*   **Goal**: S3-friendly format.
*   **Change**: `SSTableBuilder` should buffer full blocks/files before writing? 
    *   *Better*: Stream multipart uploads.
*   **Optimization**: Disable Compaction for L0->L1 (just upload L0s)?

### Prefix-Aware Compaction
*   **Goal**: Optimize for `prefix_scan(node_id)`.
*   **Status**: ✅ Prefix Bloom Filters implemented.

## 3. Concurrency Control (MVCC)

*   **Goal**: Snapshot Isolation for consistent graph traversals.
*   **Plan**:
    *   Add `sequence_number` to all keys (already done).
    *   Implement `Snapshot` struct (captures `seq_num`).
    *   Update iterators to filter by `Snapshot`.

## 4. Verification & Benchmarking

*   **Benchmarks**:
    *   `buffer_pool_bench`: Compare `quick_cache` vs `BufferPool`.
    *   `linux_io_uring`: Verify async IO benefits.

---

## Immediate Action Items

1.  **Refactor `BufferPool`**: ✅ Enable memory reuse (avoid `Vec` churn).
2.  **Benchmark**: ✅ Measure impact of `BufferPool` on read latency vs OS cache.
3.  **Zero-Copy**: ✅ Implement `BlockData` and benchmark.
4.  **Next**: Investigate Pointer Swizzling (Phase 4).

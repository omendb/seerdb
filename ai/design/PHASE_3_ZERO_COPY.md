# Phase 3: Zero-Copy Access & Block View

**Status**: DRAFT
**Date**: Nov 20, 2025

## 1. Problem Statement

Currently, the `BufferPool` implementation involves a redundant memory copy when loading blocks:
1.  **Disk -> Frame**: Data is loaded from disk into a `Frame` (owned by `BufferPool`).
2.  **Frame -> Block**: Data is copied from `Frame` (via `FrameRef`) into a `Bytes` object to create a `Block`.
3.  **Block -> Decompressed**: If compressed, data is decompressed into yet another buffer.

This copy (Step 2) is unnecessary if the `Block` could safely view the data residing in the `Frame`.

## 2. Goal

Eliminate the intermediate copy (Step 2) by making `Block` capable of holding a reference to the `Frame` directly. This paves the way for True Zero-Copy access when combined with `O_DIRECT` in the future.

## 3. Proposed Architecture

### 3.1. Block Ownership
The `Block` struct currently owns its data via `Bytes`. We need to abstract this.

```rust
enum BlockData {
    Owned(Bytes),           // Legacy/OS Cache path
    Borrowed(FrameRef),     // Zero-Copy BufferPool path
}

pub struct Block {
    data: BlockData,
    // ... metadata ...
}
```

### 3.2. Challenge: Pinning & Cache Lifetimes

If `Block` holds a `FrameRef`, the underlying frame in `BufferPool` is **pinned** for as long as the `Block` is alive.

*   **Issue**: `SSTable` maintains a `block_cache` (L1) which stores `Block` instances.
*   **Consequence**: If we cache `Block`s in L1 that hold `FrameRef`s, we are effectively pinning pages in L2 (`BufferPool`) indefinitely (until evicted from L1).
*   **Risk**: If L1 capacity >= L2 capacity, we could pin ALL frames in L2, causing `BufferPool` to run out of evictable frames (deadlock/panic).

### 3.3. Solutions

#### Option A: Ephemeral Blocks (No L1 Caching for BufferPool)
*   **Strategy**: When using `BufferPool`, **bypass** the `block_cache` (L1).
*   **Flow**: `get_page` -> `FrameRef` -> Temporary `Block` (on stack/short-lived) -> Read Data -> Drop `Block` (Unpin).
*   **Pros**: Solves pinning issue completely.
*   **Cons**: Re-parsing headers/metadata on every access (CPU overhead). No L1 cache for parsed keys.

#### Option B: Swizzling (The "LeanStore" Way)
*   **Strategy**: `Block` in L1 holds a "Swip" (Swizzled Pointer).
    *   If Frame is in BufferPool: Pointer to Frame.
    *   If Frame is evicted: Invalid/Disk Pointer.
*   **Mechanism**: `BufferPool` must invalidate L1 pointers upon eviction.
    *   This requires back-references or a centralized mapping.
    *   Complex to implement safely in Rust.

#### Option C: Hybrid (Copy for L1, View for ephemeral)
*   **Strategy**:
    *   For short-lived lookups (point reads), use a `ViewBlock` that holds `FrameRef` and is NOT cached in L1.
    *   For hot blocks, we *promote* them to L1 by performing the copy (Frame -> Bytes) to detach them from `BufferPool`.
*   **Heuristic**: Copy only on second access? Or copy small blocks?

### 3.4. Handling Compression

For compressed blocks, we **must** decompress to read values.
*   Decompression requires a destination buffer (new allocation).
*   Therefore, "Zero-Copy" is only possible for:
    1.  **Uncompressed Blocks** (L0, or specific config).
    2.  **The Input to Decompression** (Avoid copying compressed source).

Since `Block::new` immediately decompresses, the benefit of holding `FrameRef` to compressed data is limited to avoiding one small `memcpy` (4KB) before decompression.

**Conclusion**: The biggest win for Zero-Copy is with **Uncompressed Blocks**.
*   Action: Evaluate if uncompressed L0/L1 levels are viable (trade storage for CPU/Latency).

## 4. Path to O_DIRECT

1.  **Phase 2 (Done)**: Memory reuse in `BufferPool`.
2.  **Phase 3 (Next)**: `Block` wraps `FrameRef` (avoid copy).
3.  **Phase 4**: `O_DIRECT` file opening.
    *   Requires aligned buffers (512 bytes).
    *   `BufferPool` allocator must be alignment-aware.
    *   Reads go directly Disk -> Frame.

## 5. Implementation Plan

1.  **Refactor `Block`**: Trait or Enum for data source.
2.  **Update `load_block`**: Pass `FrameRef` to `Block` constructor.
3.  **Update `block_cache`**:
    *   Implement "Promote to Owned" before inserting into L1?
    *   Or disable L1 for `BufferPool` path initially to test raw L2 performance.

## 6. Recommendation

Start with **Option C (Hybrid)**:
*   `load_block` returns a `Block` that might be borrowed (FrameRef).
*   If we decide to cache it in `block_cache`, we call `.to_owned()` (deep copy/detach).
*   This allows "scan" operations (which don't populate cache) to be Zero-Copy.
*   Point lookups that hit cache pay the copy cost once (promotion).

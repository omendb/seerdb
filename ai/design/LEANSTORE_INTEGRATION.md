# LeanStore Integration Design

**Goal**: Replace `quick_cache` + OS Page Cache with a custom `BufferManager` to implement "LeanStore" principles (Paper: *LeanStore: In-Memory Data Management beyond Main Memory*, ICDE 2018).

**Status**: DRAFT (Nov 19, 2025)

## Core Principles

1.  **No Double Buffering**: We manage memory. OS cache is bypassed (`O_DIRECT`) or minimized.
2.  **Pointer Swizzling** (Phase 2): In-memory references are raw pointers; on-disk are page IDs.
3.  **Page-Oriented I/O**: Reads/Writes happen in fixed-size pages (e.g., 16KB), not arbitrary blocks.
4.  **Decentralized Epoch-Based Reclamation**: For lock-free safety.

## Architecture Fit for seerdb

`seerdb` currently uses:
- `quick_cache` for Block Cache (variable size blocks, compressed).
- `mmap` or `File::read` (OS Page Cache).

**Transition Plan**:

### Phase 1: The Buffer Manager (No Swizzling)
Implement a standard Buffer Pool first to replace `quick_cache`.

*   **Components**:
    *   `BufferPool`: Manages a fixed set of `Frames` (chunks of memory).
    *   `PageId`: `(FileId, Offset)`
    *   `FrameControl`: Atomic state (IsDirty, PinCount, Epoch).
    *   `HashTable`: Maps `PageId` -> `FrameIndex`.

*   **Eviction**:
    *   Start with **Second Chance (Clock)** or **LRU-k**.
    *   LeanStore recommends a specific randomized cooling stage, but Clock is easier for MVP.

### Phase 2: Integration
*   Modify `SSTable` reader to request `Pages` from `BufferManager` instead of `Blocks` from `quick_cache`.
*   **Challenge**: SSTable blocks are variable size (compressed).
    *   *Option A*: Keep variable blocks, just use `BufferManager` as a custom allocator/cache. (Not true LeanStore).
    *   *Option B*: Re-architect SSTables to be page-aligned (fixed 16KB). **(Preferred for long term)**.
    *   *Decision*: For Phase 1, we might wrap variable blocks in "Virtual Pages" or just enforce 16KB block size in compaction.

### Phase 3: Pointer Swizzling (Rust Hard Mode)
*   Replacing `PageId` lookups with `&Frame` references.
*   Requires `unsafe` and careful lifetime management.
*   Likely deferred until Phase 1 is stable.

## Implementation Steps (Phase 1)

1.  **`src/buffer/mod.rs`**: New module.
2.  **`BufferPool` Struct**:
    *   `frames`: `Vec<RwLock<Frame>>` (or `UnsafeCell` with atomic state).
    *   `page_table`: `ShardedLock<HashMap<PageId, FrameId>>`.
3.  **`O_DIRECT` Support**:
    *   Update `Storage` trait to support direct I/O flags.
4.  **Benchmarking**:
    *   Compare `quick_cache` vs `BufferPool` on YCSB.

## Risks
*   **Complexity**: Writing a buffer manager in Rust is hard (concurrency + safety).
*   **Performance Regression**: OS Page Cache is very good. Beating it requires tuning.
*   **Block Size Mismatch**: Existing SSTables are variable compressed blocks. Aligning them to fixed pages is a format change.

## Recommendation
Start with **Phase 1** but keep `quick_cache` as the "Page Table" for now? No, build a proper `BufferPool` struct that manages `Box<[u8]>` variants but enforce a memory limit.

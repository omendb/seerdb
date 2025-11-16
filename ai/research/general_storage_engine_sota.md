# General Storage Engine - State of the Art Research

**Date**: November 14, 2025
**Phase**: 4 of 4 (General Storage Engine for bw-tree → LeanStore pivot)
**Purpose**: Complete LeanStore architecture for experimental Mojo/Rust implementation

---

## Executive Summary

**Goal**: Design a SOTA general-purpose storage engine for the bw-tree experimental repo, **pivoting from Bw-tree to LeanStore architecture** because:
- Bw-tree is outdated (2013 design, not used outside Microsoft)
- LeanStore is SOTA (2018+, actively developed, proven performance)
- Simpler implementation (no delta chains, no complex mapping table)
- Better performance on modern hardware (NVMe, multi-core)

**Key Finding**: LeanStore's **pointer swizzling** is the biggest innovation in buffer management since the 1990s. Combined with **variable-size pages** (Umbra) and **OLC** (Optimistic Lock Coupling), it represents the state of the art for disk-based storage engines.

**Applicable to**:
- **bw-tree repo** (Mojo experiment): All findings (full LeanStore implementation)
- **seerdb-core**: Buffer management, OLC, I/O patterns (already covered in Phase 1)
- **omendb**: Buffer management concepts for L0 memory management
- **oadb**: Buffer/I/O concepts for persistence (already covered in Phase 3)

---

## Core Architecture: LeanStore

### Detailed from Phase 1, Additional Implementation Details Here

**(See `ai/research/lsm_engines_sota.md` for LeanStore overview)**

This document focuses on **implementation-level details** for building LeanStore from scratch.

---

## Component 1: Buffer Manager Deep Dive

### Page Table & Swizzling

**Data Structures**:
```rust
/// Page identifier (logical)
type PageID = u64;

/// Swizzled pointer (hot or cold)
enum Swip {
    Hot(*mut Page),   // Direct pointer to in-memory page
    Cold(u64),        // File offset for on-disk page
}

/// Page table (PID → SWIP mapping)
struct PageTable {
    mapping: HashMap<PageID, Swip>,
    lock: RwLock,  // For concurrent access
}

/// Actual page in memory
#[repr(align(4096))]  // Align to OS page size
struct Page {
    pid: PageID,
    latch: OptimisticLatch,
    data: [u8; 4096],
    dirty: AtomicBool,
}
```

**Swizzling Operations**:
```rust
impl PageTable {
    /// Get page (swizzle if cold)
    fn get_page(&mut self, pid: PageID) -> Result<*mut Page> {
        let swip = self.mapping.get(&pid)?;

        match swip {
            Swip::Hot(ptr) => Ok(*ptr),

            Swip::Cold(offset) => {
                // Load from disk
                let page = self.load_page(offset)?;

                // Allocate in buffer pool
                let ptr = self.buffer_pool.insert(page);

                // Update mapping (cold → hot)
                self.mapping.insert(pid, Swip::Hot(ptr));

                Ok(ptr)
            }
        }
    }

    /// Unswizzle page (hot → cold)
    fn unswizzle(&mut self, pid: PageID, ptr: *mut Page) {
        // 1. Flush if dirty
        if unsafe { (*ptr).dirty.load(Ordering::Acquire) } {
            self.flush_page(ptr);
        }

        // 2. Update mapping (hot → cold)
        let offset = self.page_offset(pid);
        self.mapping.insert(pid, Swip::Cold(offset));

        // 3. Deallocate from buffer pool
        self.buffer_pool.remove(ptr);
    }
}
```

**Eviction Policy (CLOCK)**:
```rust
struct BufferPool {
    pages: Vec<Option<*mut Page>>,
    clock_hand: usize,
    size: usize,
}

impl BufferPool {
    /// Find victim for eviction (CLOCK algorithm)
    fn find_victim(&mut self) -> Option<PageID> {
        loop {
            let idx = self.clock_hand;
            self.clock_hand = (self.clock_hand + 1) % self.size;

            if let Some(ptr) = self.pages[idx] {
                let page = unsafe { &*ptr };

                // Check reference bit (accessed recently?)
                if page.referenced.swap(false, Ordering::AcqRel) {
                    // Give second chance, move to next
                    continue;
                } else {
                    // Victim found!
                    return Some(page.pid);
                }
            }
        }
    }

    /// Evict page from buffer pool
    fn evict(&mut self, pid: PageID) {
        let swip = self.page_table.mapping.get(&pid).unwrap();

        if let Swip::Hot(ptr) = swip {
            self.page_table.unswizzle(pid, *ptr);
        }
    }
}
```

**Alternative Eviction: 2Q**:
```rust
// Two queues: FIFO for new pages, LRU for frequently accessed
struct TwoQueueEviction {
    fifo: VecDeque<PageID>,  // New pages (probation)
    lru: LinkedList<PageID>, // Hot pages (protected)
    max_fifo_size: usize,    // e.g., 25% of buffer pool
}

impl TwoQueueEviction {
    fn on_access(&mut self, pid: PageID) {
        // Page accessed - promote from FIFO to LRU
        if self.fifo.contains(&pid) {
            self.fifo.retain(|p| p != &pid);
            self.lru.push_back(pid);
        } else {
            // Already in LRU, move to back (most recent)
            self.lru.remove(&pid);
            self.lru.push_back(pid);
        }
    }

    fn select_victim(&mut self) -> PageID {
        // Evict from FIFO first (least recently added)
        if !self.fifo.is_empty() {
            self.fifo.pop_front().unwrap()
        } else {
            // FIFO empty, evict from LRU (least recently used)
            self.lru.pop_front().unwrap()
        }
    }
}
```

---

## Component 2: Concurrency Control - Optimistic Lock Coupling

**Optimistic Latch Implementation**:
```rust
struct OptimisticLatch {
    version_lock: AtomicU64,
}

const LOCK_BIT: u64 = 1 << 63;

impl OptimisticLatch {
    fn new() -> Self {
        Self { version_lock: AtomicU64::new(0) }
    }

    /// Read version (optimistic read start)
    fn read_version(&self) -> u64 {
        self.version_lock.load(Ordering::Acquire)
    }

    /// Validate version hasn't changed (optimistic read end)
    fn validate(&self, old_version: u64) -> bool {
        let current = self.version_lock.load(Ordering::Acquire);
        // Valid if: version unchanged AND not locked
        current == old_version && (current & LOCK_BIT) == 0
    }

    /// Try to acquire exclusive lock (for writes)
    fn try_lock(&self) -> Option<u64> {
        let version = self.read_version();

        // Already locked?
        if (version & LOCK_BIT) != 0 {
            return None;
        }

        // Try CAS to set lock bit
        let locked = version | LOCK_BIT;
        match self.version_lock.compare_exchange(
            version,
            locked,
            Ordering::Acquire,
            Ordering::Relaxed
        ) {
            Ok(_) => Some(version),
            Err(_) => None,
        }
    }

    /// Release lock and increment version (write complete)
    fn unlock(&self, old_version: u64) {
        let new_version = (old_version & !LOCK_BIT) + 1;
        self.version_lock.store(new_version, Ordering::Release);
    }

    /// Check if currently locked
    fn is_locked(&self) -> bool {
        (self.version_lock.load(Ordering::Acquire) & LOCK_BIT) != 0
    }
}
```

**Optimistic B+-Tree Lookup**:
```rust
impl BPlusTree {
    fn lookup(&self, key: &Key) -> Option<Value> {
        loop {
            // 1. Read root (optimistic)
            let (root_pid, root_version) = self.read_node_optimistic(self.root);

            // 2. Traverse to leaf (optimistic, no locks)
            let leaf_pid = self.traverse_to_leaf(root_pid, key);
            let (leaf_page, leaf_version) = self.read_node_optimistic(leaf_pid);

            // 3. Search in leaf
            let value = leaf_page.search(key);

            // 4. Validate versions (no concurrent modifications?)
            if self.validate_path(root_pid, root_version, leaf_pid, leaf_version) {
                return value;
            }

            // Validation failed, retry
        }
    }

    fn read_node_optimistic(&self, pid: PageID) -> (*mut Page, u64) {
        let page = self.buffer_manager.get_page(pid);
        let version = unsafe { (*page).latch.read_version() };
        (page, version)
    }

    fn validate_path(&self, root_pid: PageID, root_version: u64,
                     leaf_pid: PageID, leaf_version: u64) -> bool {
        let root_page = self.buffer_manager.get_page(root_pid);
        let leaf_page = self.buffer_manager.get_page(leaf_pid);

        unsafe {
            (*root_page).latch.validate(root_version) &&
            (*leaf_page).latch.validate(leaf_version)
        }
    }
}
```

**Optimistic B+-Tree Insert**:
```rust
impl BPlusTree {
    fn insert(&mut self, key: Key, value: Value) -> Result<()> {
        loop {
            // 1. Traverse optimistically
            let path = self.traverse_with_path(self.root, &key);

            // 2. Try lock leaf
            let leaf = path.last().unwrap();
            if let Some(version) = leaf.try_lock() {
                // 3. Insert into leaf
                match leaf.insert_into_page(key, value) {
                    Ok(_) => {
                        // Success, unlock and return
                        leaf.unlock(version);
                        return Ok(());
                    }

                    Err(PageFull) => {
                        // Need to split, lock parent too
                        if let Some(parent) = self.lock_parent(&path) {
                            self.split_leaf(leaf, parent);
                            parent.unlock();
                            leaf.unlock(version);
                            return Ok(());
                        } else {
                            // Parent lock failed, retry
                            leaf.unlock(version);
                            continue;
                        }
                    }
                }
            } else {
                // Lock failed (concurrent modification), retry
                continue;
            }
        }
    }
}
```

---

## Component 3: Memory Reclamation

**Epoch-Based Reclamation** (safer than hazard pointers for storage engines):

```rust
struct EpochManager {
    global_epoch: AtomicU64,
    thread_epochs: Vec<AtomicU64>,
    garbage: Vec<Vec<*mut Page>>,  // Per-epoch garbage lists
}

impl EpochManager {
    /// Thread enters epoch (critical section)
    fn enter(&self, thread_id: usize) {
        let current_epoch = self.global_epoch.load(Ordering::Acquire);
        self.thread_epochs[thread_id].store(current_epoch, Ordering::Release);
    }

    /// Thread exits epoch (safe to reclaim)
    fn exit(&self, thread_id: usize) {
        self.thread_epochs[thread_id].store(u64::MAX, Ordering::Release);
        self.try_reclaim();
    }

    /// Advance global epoch periodically
    fn advance_epoch(&self) {
        self.global_epoch.fetch_add(1, Ordering::AcqRel);
    }

    /// Schedule page for deferred deletion
    fn retire(&mut self, ptr: *mut Page) {
        let current_epoch = self.global_epoch.load(Ordering::Acquire);
        self.garbage[current_epoch as usize % 3].push(ptr);
    }

    /// Reclaim memory from safe epochs
    fn try_reclaim(&mut self) {
        let current = self.global_epoch.load(Ordering::Acquire);

        // Find minimum epoch across all threads
        let min_epoch = self.thread_epochs.iter()
            .map(|e| e.load(Ordering::Acquire))
            .min()
            .unwrap_or(current);

        // Safe to reclaim epochs < min_epoch
        for epoch in 0..min_epoch {
            for ptr in self.garbage[epoch as usize % 3].drain(..) {
                unsafe { drop(Box::from_raw(ptr)); }
            }
        }
    }
}
```

**Usage in B+-Tree**:
```rust
impl BPlusTree {
    fn delete_page(&mut self, page: *mut Page) {
        // Don't free immediately (other threads may still access)
        self.epoch_manager.retire(page);
    }

    fn lookup_with_epoch(&self, key: &Key) -> Option<Value> {
        // Enter epoch (protect from reclamation)
        self.epoch_manager.enter(self.thread_id);

        // Perform lookup (pages won't be freed)
        let result = self.lookup(key);

        // Exit epoch (allow reclamation)
        self.epoch_manager.exit(self.thread_id);

        result
    }
}
```

---

## Component 4: I/O Subsystem

### io_uring (Linux)

**Setup**:
```rust
use io_uring::{IoUring, opcode, types};

struct AsyncIO {
    ring: IoUring,
    fd: RawFd,
}

impl AsyncIO {
    fn new(path: &Path, queue_depth: u32) -> Result<Self> {
        let ring = IoUring::new(queue_depth)?;
        let fd = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_DIRECT)  // Direct I/O
            .open(path)?
            .into_raw_fd();

        Ok(Self { ring, fd })
    }
}
```

**Single Page Read**:
```rust
impl AsyncIO {
    fn read_page(&mut self, offset: u64, buffer: &mut [u8; 4096]) -> Result<()> {
        // Submit read operation
        let read_op = opcode::Read::new(
            types::Fd(self.fd),
            buffer.as_mut_ptr(),
            4096
        ).offset(offset);

        unsafe {
            self.ring.submission()
                .push(&read_op.build())
                .expect("submission queue full");
        }

        // Submit to kernel
        self.ring.submit_and_wait(1)?;

        // Get completion
        let cqe = self.ring.completion()
            .next()
            .expect("completion queue empty");

        if cqe.result() < 0 {
            return Err(io::Error::from_raw_os_error(-cqe.result()));
        }

        Ok(())
    }
}
```

**Batch Reads** (key for performance):
```rust
impl AsyncIO {
    fn batch_read(&mut self, requests: &[(u64, &mut [u8; 4096])]) -> Result<()> {
        // Submit all reads
        for (offset, buffer) in requests {
            let read_op = opcode::Read::new(
                types::Fd(self.fd),
                buffer.as_mut_ptr(),
                4096
            ).offset(*offset);

            unsafe {
                self.ring.submission()
                    .push(&read_op.build())?;
            }
        }

        // Submit batch
        self.ring.submit()?;

        // Wait for all completions
        for _ in 0..requests.len() {
            let cqe = self.ring.completion().next().unwrap();
            if cqe.result() < 0 {
                return Err(io::Error::from_raw_os_error(-cqe.result()));
            }
        }

        Ok(())
    }
}
```

**Write with fsync** (for WAL durability):
```rust
impl AsyncIO {
    fn write_and_sync(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        // 1. Submit write
        let write_op = opcode::Write::new(
            types::Fd(self.fd),
            data.as_ptr(),
            data.len() as u32
        ).offset(offset);

        unsafe { self.ring.submission().push(&write_op.build())?; }

        // 2. Submit fsync (wait for write to complete first)
        let fsync_op = opcode::Fsync::new(types::Fd(self.fd));
        unsafe { self.ring.submission().push(&fsync_op.build())?; }

        // 3. Submit batch
        self.ring.submit_and_wait(2)?;

        // 4. Check completions
        for _ in 0..2 {
            let cqe = self.ring.completion().next().unwrap();
            if cqe.result() < 0 {
                return Err(io::Error::from_raw_os_error(-cqe.result()));
            }
        }

        Ok(())
    }
}
```

### macOS Fallback (POSIX AIO)

```rust
#[cfg(target_os = "macos")]
struct AsyncIO {
    fd: RawFd,
}

#[cfg(target_os = "macos")]
impl AsyncIO {
    fn read_page(&mut self, offset: u64, buffer: &mut [u8; 4096]) -> Result<()> {
        // Synchronous fallback on macOS (no io_uring)
        let mut file = unsafe { File::from_raw_fd(self.fd) };
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buffer)?;
        std::mem::forget(file);  // Don't close FD
        Ok(())
    }

    fn write_and_sync(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        let mut file = unsafe { File::from_raw_fd(self.fd) };
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        file.sync_data()?;
        std::mem::forget(file);
        Ok(())
    }
}
```

---

## Component 5: Index Structures - B+-Tree with OLC

**Node Structure**:
```rust
const FANOUT: usize = 256;  // For 4KB pages

#[repr(C)]
struct BTreeNode {
    latch: OptimisticLatch,
    num_keys: u16,
    is_leaf: bool,
    keys: [Key; FANOUT],
    // Internal: PageID children
    // Leaf: Value data
    payload: NodePayload,
}

enum NodePayload {
    Internal([PageID; FANOUT + 1]),
    Leaf([Value; FANOUT]),
}
```

**Search**:
```rust
impl BTreeNode {
    /// Binary search for key position
    fn find_position(&self, key: &Key) -> usize {
        let num_keys = self.num_keys as usize;
        self.keys[..num_keys]
            .binary_search(key)
            .unwrap_or_else(|pos| pos)
    }

    /// Find child to traverse (internal node)
    fn find_child(&self, key: &Key) -> PageID {
        let pos = self.find_position(key);
        match &self.payload {
            NodePayload::Internal(children) => children[pos],
            _ => panic!("find_child called on leaf"),
        }
    }

    /// Search for value (leaf node)
    fn search(&self, key: &Key) -> Option<Value> {
        let pos = self.find_position(key);
        match &self.payload {
            NodePayload::Leaf(values) if pos < self.num_keys as usize => {
                if self.keys[pos] == *key {
                    Some(values[pos])
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
```

**Split** (when node full):
```rust
impl BTreeNode {
    fn split(&mut self) -> (Key, BTreeNode) {
        let mid = self.num_keys as usize / 2;
        let split_key = self.keys[mid];

        // Create new sibling node
        let mut sibling = BTreeNode::new(self.is_leaf);

        // Move right half to sibling
        let right_count = self.num_keys as usize - mid - 1;
        sibling.keys[..right_count].copy_from_slice(&self.keys[mid+1..self.num_keys as usize]);
        sibling.num_keys = right_count as u16;

        // Update this node
        self.num_keys = mid as u16;

        match (&mut self.payload, &mut sibling.payload) {
            (NodePayload::Internal(my_children), NodePayload::Internal(sibling_children)) => {
                sibling_children[..=right_count]
                    .copy_from_slice(&my_children[mid+1..=self.num_keys as usize]);
            }
            (NodePayload::Leaf(my_values), NodePayload::Leaf(sibling_values)) => {
                sibling_values[..right_count]
                    .copy_from_slice(&my_values[mid+1..self.num_keys as usize]);
            }
            _ => panic!("split type mismatch"),
        }

        (split_key, sibling)
    }
}
```

---

## Component 6: WAL & Recovery

**WAL Record Types**:
```rust
enum WALRecord {
    Begin { txn_id: u64 },
    Insert { txn_id: u64, pid: PageID, key: Key, value: Value },
    Update { txn_id: u64, pid: PageID, key: Key, old: Value, new: Value },
    Delete { txn_id: u64, pid: PageID, key: Key },
    Commit { txn_id: u64 },
    Abort { txn_id: u64 },
    Checkpoint { lsn: u64 },
}

struct WAL {
    file: File,
    buffer: Vec<u8>,
    lsn: AtomicU64,  // Log Sequence Number
}
```

**Append (Autonomous Commit)**:
```rust
impl WAL {
    fn append(&mut self, record: WALRecord) -> Result<u64> {
        // 1. Assign LSN
        let lsn = self.lsn.fetch_add(1, Ordering::SeqCst);

        // 2. Serialize record
        let bytes = bincode::serialize(&record)?;

        // 3. Write to file (no batching, immediate flush for NVMe)
        self.file.write_all(&bytes)?;
        self.file.sync_data()?;  // fsync

        Ok(lsn)
    }
}
```

**Recovery**:
```rust
impl WAL {
    fn recover(&mut self, buffer_manager: &mut BufferManager) -> Result<()> {
        // 1. Find last checkpoint
        let checkpoint_lsn = self.find_last_checkpoint()?;

        // 2. Replay from checkpoint
        self.file.seek(SeekFrom::Start(checkpoint_lsn))?;

        let mut active_txns = HashSet::new();

        for record in self.read_records() {
            match record {
                WALRecord::Begin { txn_id } => {
                    active_txns.insert(txn_id);
                }

                WALRecord::Insert { pid, key, value, .. } => {
                    let page = buffer_manager.get_page(pid)?;
                    unsafe { (*page).insert(key, value); }
                }

                WALRecord::Commit { txn_id } => {
                    active_txns.remove(&txn_id);
                }

                WALRecord::Abort { txn_id } => {
                    // Undo operations (needs undo log)
                    self.undo_transaction(txn_id)?;
                    active_txns.remove(&txn_id);
                }

                _ => {}
            }
        }

        // 3. Abort incomplete transactions
        for txn_id in active_txns {
            self.undo_transaction(txn_id)?;
        }

        Ok(())
    }
}
```

---

## Synthesis: LeanStore Implementation Roadmap

### Minimal Working System

**Core Components** (in order):
1. Page structure (4KB aligned)
2. Page table (PID → SWIP)
3. Buffer pool (CLOCK eviction)
4. Optimistic latch (version-based)
5. B+-Tree (insert, search, delete)
6. I/O layer (io_uring on Linux, fallback on macOS)
7. WAL (append, recovery)

**Timeline** (for bw-tree Mojo/Rust experiment):
- **Weeks 1-2**: Page table, buffer pool, swizzling
- **Week 3**: Optimistic latches, B+-Tree structure
- **Week 4**: B+-Tree operations (insert, search)
- **Week 5**: I/O layer, persistence
- **Week 6**: WAL, recovery
- **Weeks 7-8**: Testing, benchmarking vs RocksDB/SQLite

---

## Key Takeaways for Each Repository

### bw-tree (Experimental Mojo/Rust)
**Focus**: Implement LeanStore from scratch for learning
- ✅ Complete LeanStore architecture (all 6 components)
- ✅ Pointer swizzling (replaces Bw-tree mapping table)
- ✅ OLC (replaces lock-free delta chains)
- ✅ Variable-size pages (future enhancement)
- ✅ io_uring integration (Linux)
- ✅ Mojo vs Rust comparison (is Mojo viable for systems programming?)

### seerdb-core (General-Purpose LSM)
**Focus**: Production LSM with LeanStore buffer manager
- ✅ LeanStore buffer manager (from Phase 1)
- ✅ OLC for concurrent access
- ✅ io_uring async I/O
- ✅ WAL with autonomous commits
- ⏭️ Future: Variable-size pages

### omendb (Cloud Vector Database)
**Focus**: L0 memory management using buffer manager concepts
- ✅ Buffer management patterns for L0 HNSW graphs
- ✅ Eviction policies (CLOCK/2Q) for hot vector eviction
- ❌ No B+-Tree needed (LSM-VEC uses HNSW graphs)

### oadb (Embedded Vector Database)
**Focus**: Simple persistence, no complex buffer management
- ✅ WAL concepts for incremental save
- ✅ I/O optimization for mmap-friendly format
- ❌ No buffer manager (all in-memory)
- ❌ No OLC (single-threaded embedded)

---

## Open Questions to Resolve

1. **Mojo for storage engines**:
   - Is Mojo mature enough for systems programming?
   - **Approach**: Build minimal prototype, compare with Rust
   - **Timeline**: 2 months maximum, fallback to Rust if blocked

2. **Variable-size pages implementation**:
   - Buddy allocator for size classes?
   - **Lean towards**: Start with fixed 4KB, add variable sizes later

3. **Eviction policy**:
   - CLOCK (simple) vs 2Q (better) vs LRU-K (best)?
   - **Lean towards**: CLOCK first, upgrade to 2Q if needed

4. **WAL vs No-WAL**:
   - Always use WAL, or make it optional?
   - **Lean towards**: Always WAL for durability guarantees

---

## References

**LeanStore & Related**:
- Leis et al., "LeanStore: In-Memory Data Management Beyond Main Memory", ICDE 2018
- Alhomssi & Leis, "Scalable and Robust Snapshot Isolation", VLDB 2023
- Nguyen et al., "Autonomous Commits for High Throughput", PACMMOD 2025
- Source: https://github.com/leanstore/leanstore (MIT)

**Umbra**:
- Neumann & Freitag, "Umbra: A Disk-Based System with In-Memory Performance", CIDR 2020

**Concurrency**:
- Leis et al., "The ART of Practical Synchronization", DaMoN 2016 (OLC)
- Graefe, "A Survey of B-Tree Locking Techniques", ACM TODS 2010

**I/O**:
- Axboe, "Efficient I/O with io_uring", 2019
- Linux kernel docs: https://kernel.dk/io_uring.pdf

**General Storage Engines**:
- Graefe, "Modern B-Tree Techniques", Foundations and Trends in Databases 2011
- Arpaci-Dusseau, "Operating Systems: Three Easy Pieces", Chapter 40 (File Systems)

---

**END OF GENERAL STORAGE ENGINE SOTA RESEARCH**

This document provides complete LeanStore implementation guide for bw-tree experimental repo.
All 4 phases of research now complete!

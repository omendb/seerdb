use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use parking_lot::{RwLock, Mutex};
use dashmap::DashMap;
use crate::buffer::eviction::{EvictionPolicy, ClockPolicy, FrameId};
use std::fmt;

pub type FileId = u64;
pub type BlockId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId {
    pub file_id: FileId,
    pub offset: BlockId, 
}

#[derive(Debug, Clone)]
pub struct BufferPoolOptions {
    pub capacity_bytes: usize,
    pub frame_size: usize,
}

impl Default for BufferPoolOptions {
    fn default() -> Self {
        Self {
            capacity_bytes: 128 * 1024 * 1024, // 128MB
            frame_size: 16 * 1024, // 16KB
        }
    }
}

/// Thread-safe reference to a pinned frame.
pub struct FrameRef {
    pool: Arc<BufferPool>,
    page_id: PageId,
    frame_id: FrameId,
    // We hold a read guard on the data to prevent eviction while we hold this Ref.
    // In a more advanced lock-free system, the pin_count is sufficient.
    // Here, because eviction writes to 'data', we technically need to ensure 'data' isn't mutated.
    // However, eviction checks pin_count. If pin_count > 0, it won't touch data.
    // So we don't need to hold the RwLock guard, just the pin.
}

impl FrameRef {
    pub fn data(&self) -> &[u8] {
        // SAFETY: 
        // 1. We hold a pin count > 0 (ensured by constructor and Drop).
        // 2. Eviction policy checks pin count before repurposing frame.
        // 3. Therefore, the data in this frame belongs to self.page_id and won't change.
        // 4. We take a read lock just to be safe against any internal mutability or race 
        //    during the initial loading phase, though technically once pinned and loaded, it's stable.
        //    Actually, to return &[u8], we need to bypass the RwLock or return a Guard.
        //    Returning a Guard is safer.
        panic!("Use get_data() which returns a guard");
    }

    pub fn get_data(&self) -> parking_lot::RwLockReadGuard<'_, Vec<u8>> {
        self.pool.frames[self.frame_id].data.read()
    }

    /// Unsafe access to data slice without lock guard.
    ///
    /// # Safety
    /// Caller must ensure that:
    /// 1. The frame is pinned (which `FrameRef` ensures).
    /// 2. The data is not being mutated concurrently (guaranteed for SSTables as they are immutable).
    /// 3. The `Vec` does not reallocate (guaranteed as we only resize on initial load).
    pub unsafe fn data_unchecked(&self) -> &[u8] {
        // Access the data field directly through the raw pointer of the RwLock
        // This is highly unsafe and relies on internal implementation details of RwLock or
        // requires us to trust that the data ptr inside Vec is stable.
        //
        // A safer "unsafe" way is to acquire the read lock, get the pointer, and trick the compiler,
        // but we want to avoid the atomic overhead of the lock if possible.
        //
        // For now, let's stick to the "safest" unsafe way: 
        // We know the frame data is wrapped in RwLock. We can't easily bypass it without
        // getting a raw pointer to the content.
        //
        // Let's try to be slightly disciplined: this function is used where we *know*
        // we have exclusive or immutable access.
        
        // We will take a read lock for a split second to get the slice, then extend its lifetime.
        // This is "technically" UB if someone writes, but we know they won't.
        let guard = self.get_data();
        let ptr = guard.as_ptr();
        let len = guard.len();
        std::slice::from_raw_parts(ptr, len)
    }
}

impl Clone for FrameRef {
    fn clone(&self) -> Self {
        // Increment pin count to account for the new reference
        self.pool.pin_frame(self.frame_id);
        Self {
            pool: self.pool.clone(),
            page_id: self.page_id,
            frame_id: self.frame_id,
        }
    }
}

impl Drop for FrameRef {
    fn drop(&mut self) {
        self.pool.unpin(self.frame_id);
    }
}

impl fmt::Debug for FrameRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FrameRef(page={:?}, frame={})", self.page_id, self.frame_id)
    }
}

struct FrameHeader {
    pin_count: AtomicUsize,
    is_dirty: AtomicBool,
    // The page_id currently stored here. 
    // We use a Lock because changing ownership (eviction) is complex.
    page_id: RwLock<Option<PageId>>, 
}

struct FrameSlot {
    header: FrameHeader,
    data: RwLock<Vec<u8>>,
}

pub struct BufferPool {
    frames: Vec<FrameSlot>,
    page_table: DashMap<PageId, FrameId>,
    free_list: Mutex<Vec<FrameId>>,
    eviction: Box<dyn EvictionPolicy>,
    #[allow(dead_code)] // Kept for future use (e.g. resizing)
    options: BufferPoolOptions,
}

impl BufferPool {
    pub fn new(options: BufferPoolOptions) -> Arc<Self> {
        let num_frames = options.capacity_bytes / options.frame_size;
        let mut frames = Vec::with_capacity(num_frames);
        let mut free_list = Vec::with_capacity(num_frames);

        for i in 0..num_frames {
            frames.push(FrameSlot {
                header: FrameHeader {
                    pin_count: AtomicUsize::new(0),
                    is_dirty: AtomicBool::new(false),
                    page_id: RwLock::new(None),
                },
                data: RwLock::new(vec![0u8; options.frame_size]), 
            });
            free_list.push(i);
        }

        Arc::new(Self {
            frames,
            page_table: DashMap::new(),
            free_list: Mutex::new(free_list),
            eviction: Box::new(ClockPolicy::new(num_frames)),
            options,
        })
    }

    pub fn get_page<F, E>(self: &Arc<Self>, page_id: PageId, loader: F) -> Result<FrameRef, E>
    where
        F: FnOnce(&mut Vec<u8>) -> Result<(), E>,
    {
        // 1. Try find in cache
        if let Some(frame_ref) = self.lookup(page_id) {
            return Ok(frame_ref);
        }

        // 2. Miss - Allocate frame
        let frame_id = match self.allocate_frame() {
            Some(id) => id,
            None => return Err(self.make_capacity_error::<E>()), // TODO: defined error
        };

        // CRITICAL: Pin frame immediately to prevent eviction from stealing it
        // while we are loading data. allocate_frame returns a frame with pin_count=0.
        self.pin_frame(frame_id);

        // 3. Load data
        // We own frame_id exclusively (it was pulled from free list or eviction).
        // But we need to be careful: someone else might have loaded `page_id` in parallel.
        // Double check page_table.
        if let Some(frame_ref) = self.lookup(page_id) {
            self.free_frame(frame_id);
            self.unpin(frame_id); // Revert our pin
            return Ok(frame_ref);
        }

        // Load into existing buffer
        // Acquire write lock on data - strictly safe as we own the frame (it's not in page_table yet)
        {
            let slot = &self.frames[frame_id];
            let mut data_guard = slot.data.write();
            
            // Execute loader with mutable access to the buffer
            match loader(&mut *data_guard) {
                Ok(_) => {},
                Err(e) => {
                    // Load failed - free frame and return error
                    // We don't need to clear buffer, it will be overwritten next time
                    drop(data_guard);
                    self.free_frame(frame_id);
                    self.unpin(frame_id); // Revert our pin
                    return Err(e);
                }
            }
            
            // 4. Install metadata
            let mut pid_guard = slot.header.page_id.write();
            *pid_guard = Some(page_id);
            
            // slot.header.pin_count.store(1, Ordering::SeqCst); // Pin immediately
            // ALREADY PINNED above. pin_count is 1.
            slot.header.is_dirty.store(false, Ordering::SeqCst);
        }

        // 5. Publish to page table
        self.page_table.insert(page_id, frame_id);
        
        // 6. Mark access for eviction policy
        self.eviction.access(frame_id);

        Ok(FrameRef {
            pool: self.clone(),
            page_id,
            frame_id,
        })
    }

    fn lookup(self: &Arc<Self>, page_id: PageId) -> Option<FrameRef> {
        if let Some(entry) = self.page_table.get(&page_id) {
            let frame_id = *entry.value();
            
            // Optimistic pin
            self.pin_frame(frame_id);
            
            // Verify it's still the right page (eviction race check)
            let slot = &self.frames[frame_id];
            let current_pid = slot.header.page_id.read();
            if *current_pid == Some(page_id) {
                 self.eviction.access(frame_id);
                 return Some(FrameRef {
                     pool: self.clone(),
                     page_id,
                     frame_id,
                 });
            }
            
            // Wrong page (was evicted and repurposed before we pinned)
            self.unpin(frame_id);
        }
        None
    }

    fn allocate_frame(&self) -> Option<FrameId> {
        // 1. Free list
        {
            let mut free = self.free_list.lock();
            if let Some(id) = free.pop() {
                return Some(id);
            }
        }

        // 2. Eviction
        let max_attempts = self.frames.len() * 2;
        let mut attempts = 0;
        
        while attempts < max_attempts {
            attempts += 1;
            if let Some(victim_id) = self.eviction.evict() {
                let slot = &self.frames[victim_id];
                
                // Check pin count
                if slot.header.pin_count.load(Ordering::SeqCst) > 0 {
                    continue; 
                }

                // Lock page_id to claim ownership
                let mut pid_guard = slot.header.page_id.write();
                
                // Check pin count again under lock
                if slot.header.pin_count.load(Ordering::SeqCst) > 0 {
                    continue;
                }

                // Remove from page table
                if let Some(old_pid) = *pid_guard {
                    self.page_table.remove(&old_pid);
                }
                
                *pid_guard = None; // Mark as invalid/being setup

                // TODO: Flush if dirty

                return Some(victim_id);
            }
        }
        
        None // Failed to find victim
    }

    fn pin_frame(&self, frame_id: FrameId) {
        self.frames[frame_id].header.pin_count.fetch_add(1, Ordering::SeqCst);
    }

    fn unpin(&self, frame_id: FrameId) {
        self.frames[frame_id].header.pin_count.fetch_sub(1, Ordering::SeqCst);
    }

    fn free_frame(&self, frame_id: FrameId) {
        let mut free = self.free_list.lock();
        free.push(frame_id);
    }
    
    // Helper to forge error type
    fn make_capacity_error<E>(&self) -> E {
        // Hack: assuming E can be created from string or is specific.
        // For now just panic since we don't have the Error trait bound visible here
        // In real code we'd require E: From<BufferError>
        panic!("Buffer pool full");
    }
}

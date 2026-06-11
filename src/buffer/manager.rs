//! Buffer pool manager.
//!
//! The buffer manager allocates a fixed pool of page frames and provides
//! methods to fetch, pin, and evict pages. It uses a simple clock algorithm
//! for eviction.

use crate::btree::node::PAGE_SIZE;
use crate::buffer::frame::Frame;
use crate::buffer::guard::{GuardAccess, PageGuard};
use std::collections::HashMap;

/// Statistics about the buffer pool.
#[derive(Debug, Clone, Default)]
pub struct BufferStats {
    /// Total number of frames.
    pub total_frames: usize,
    /// Number of free frames.
    pub free_frames: usize,
    /// Number of pinned frames.
    pub pinned_frames: usize,
    /// Number of dirty frames.
    pub dirty_frames: usize,
    /// Total page reads (cache misses).
    pub reads: u64,
    /// Total page writes (evictions of dirty pages).
    pub writes: u64,
    /// Cache hits.
    pub hits: u64,
}

/// Buffer pool manager.
///
/// Manages a fixed-size pool of page frames in memory. Pages are loaded
/// on demand and evicted using a clock algorithm when the pool is full.
pub struct BufferManager {
    /// Array of buffer frames.
    frames: Vec<Frame>,
    /// Map from page ID to frame index (for cache lookup).
    page_map: HashMap<u64, usize>,
    /// Clock hand for eviction (index into frames).
    clock_hand: usize,
    /// Statistics.
    stats: BufferStats,
}

impl BufferManager {
    /// Create a new buffer manager with the given capacity in bytes.
    ///
    /// The capacity is rounded down to a multiple of PAGE_SIZE.
    pub fn new(capacity_bytes: usize) -> Self {
        let num_frames = capacity_bytes / PAGE_SIZE;
        let frames = (0..num_frames).map(|_| Frame::new_empty()).collect();

        Self {
            frames,
            page_map: HashMap::new(),
            clock_hand: 0,
            stats: BufferStats {
                total_frames: num_frames,
                free_frames: num_frames,
                ..Default::default()
            },
        }
    }

    /// Number of frames in the buffer pool.
    pub fn capacity(&self) -> usize {
        self.frames.len()
    }

    /// Get buffer pool statistics.
    pub fn stats(&self) -> &BufferStats {
        &self.stats
    }

    /// Fetch a page into the buffer pool and return a guard.
    ///
    /// If the page is already in the pool, return a guard to it (cache hit).
    /// Otherwise, load it from the provided data and return a guard.
    ///
    /// The `access` parameter controls whether the guard allows writes.
    pub fn fetch(
        &mut self,
        page_id: u64,
        data: &[u8; PAGE_SIZE],
        access: GuardAccess,
    ) -> PageGuard {
        // Check if the page is already in the pool.
        if let Some(&frame_idx) = self.page_map.get(&page_id) {
            let frame = &mut self.frames[frame_idx];
            if !frame.is_free() {
                frame.pin();
                self.stats.hits += 1;
                if access == GuardAccess::Write {
                    frame.mark_dirty();
                }
                return PageGuard::new(frame_idx, page_id, access);
            }
        }

        // Cache miss - need to load the page.
        self.stats.reads += 1;

        // Find a free frame or evict one.
        let frame_idx = self.find_free_frame();

        // Load the page into the frame.
        let frame = &mut self.frames[frame_idx];
        frame.load(page_id, data);
        frame.pin();
        if access == GuardAccess::Write {
            frame.mark_dirty();
        }

        // Update the page map.
        self.page_map.insert(page_id, frame_idx);
        self.stats.free_frames -= 1;

        PageGuard::new(frame_idx, page_id, access)
    }

    /// Get a reference to the data in a frame.
    pub fn frame_data(&self, guard: &PageGuard) -> &[u8; PAGE_SIZE] {
        &self.frames[guard.frame_index()].data
    }

    /// Get a mutable reference to the data in a frame.
    pub fn frame_data_mut(&mut self, guard: &PageGuard) -> &mut [u8; PAGE_SIZE] {
        &mut self.frames[guard.frame_index()].data
    }

    /// Mark a page as dirty (has been modified).
    pub fn mark_dirty(&mut self, page_id: u64) {
        if let Some(&frame_idx) = self.page_map.get(&page_id) {
            self.frames[frame_idx].mark_dirty();
        }
    }

    /// Unpin a page (allow it to be evicted).
    pub fn unpin(&mut self, page_id: u64) {
        if let Some(&frame_idx) = self.page_map.get(&page_id) {
            self.frames[frame_idx].unpin();
        }
    }

    /// Flush a dirty page to the provided buffer.
    ///
    /// Returns the page data if the page was dirty, None otherwise.
    pub fn flush(&mut self, page_id: u64) -> Option<Box<[u8; PAGE_SIZE]>> {
        if let Some(&frame_idx) = self.page_map.get(&page_id) {
            let frame = &mut self.frames[frame_idx];
            if frame.is_dirty() {
                let data = frame.data.clone();
                frame.mark_clean();
                self.stats.writes += 1;
                return Some(data);
            }
        }
        None
    }

    /// Flush all dirty pages.
    ///
    /// Returns a vector of (page_id, data) for all flushed pages.
    pub fn flush_all(&mut self) -> Vec<(u64, Box<[u8; PAGE_SIZE]>)> {
        let mut flushed = Vec::new();

        for (page_id, &frame_idx) in self.page_map.iter() {
            let frame = &mut self.frames[frame_idx];
            if frame.is_dirty() {
                let data = frame.data.clone();
                frame.mark_clean();
                self.stats.writes += 1;
                flushed.push((*page_id, data));
            }
        }

        flushed
    }

    /// Remove a page from the buffer pool.
    pub fn evict(&mut self, page_id: u64) {
        if let Some(frame_idx) = self.page_map.remove(&page_id) {
            self.frames[frame_idx].clear();
            self.stats.free_frames += 1;
        }
    }

    /// Find a free frame or evict one using the clock algorithm.
    fn find_free_frame(&mut self) -> usize {
        // First, look for a free frame.
        for (i, frame) in self.frames.iter().enumerate() {
            if frame.is_free() {
                return i;
            }
        }

        // No free frames - use clock algorithm to evict.
        self.clock_evict()
    }

    /// Clock algorithm for eviction.
    ///
    /// Scans frames in a circular manner. If a frame is referenced,
    /// clear the reference bit and move on. Otherwise, evict it.
    fn clock_evict(&mut self) -> usize {
        let len = self.frames.len();

        loop {
            let idx = self.clock_hand;
            self.clock_hand = (self.clock_hand + 1) % len;

            let frame = &mut self.frames[idx];

            // Skip pinned frames.
            if frame.pinned {
                continue;
            }

            // If referenced, clear the bit and move on.
            if frame.referenced {
                frame.referenced = false;
                continue;
            }

            // Found a victim - evict it.
            if let Some(old_page_id) = frame.page_id {
                self.page_map.remove(&old_page_id);
            }

            // If dirty, we'd need to flush it. For now, just clear it.
            // In the full implementation, this would trigger a write-back.
            frame.clear();
            self.stats.free_frames += 1;

            return idx;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_manager_new() {
        let bm = BufferManager::new(4096 * 10); // 10 frames
        assert_eq!(bm.capacity(), 10);
        assert_eq!(bm.stats().total_frames, 10);
        assert_eq!(bm.stats().free_frames, 10);
    }

    #[test]
    fn test_fetch_and_hit() {
        let mut bm = BufferManager::new(4096 * 2);
        let data = [42u8; PAGE_SIZE];

        // First fetch - cache miss.
        let guard = bm.fetch(1, &data, GuardAccess::Read);
        assert_eq!(bm.stats().reads, 1);
        assert_eq!(bm.stats().hits, 0);
        bm.unpin(guard.page_id());

        // Second fetch - cache hit.
        let guard = bm.fetch(1, &data, GuardAccess::Read);
        assert_eq!(bm.stats().reads, 1);
        assert_eq!(bm.stats().hits, 1);
        bm.unpin(guard.page_id());
    }

    #[test]
    fn test_eviction() {
        let mut bm = BufferManager::new(4096 * 2); // Only 2 frames
        let data1 = [1u8; PAGE_SIZE];
        let data2 = [2u8; PAGE_SIZE];
        let data3 = [3u8; PAGE_SIZE];

        // Fill the buffer.
        let g1 = bm.fetch(1, &data1, GuardAccess::Read);
        let g2 = bm.fetch(2, &data2, GuardAccess::Read);
        assert_eq!(bm.stats().free_frames, 0);

        // Unpin pages to allow eviction.
        bm.unpin(g1.page_id());
        bm.unpin(g2.page_id());

        // Fetch a new page - should evict one.
        let g3 = bm.fetch(3, &data3, GuardAccess::Read);
        assert_eq!(bm.stats().reads, 3);
        bm.unpin(g3.page_id());
    }

    #[test]
    fn test_dirty_page() {
        let mut bm = BufferManager::new(4096);
        let data = [0u8; PAGE_SIZE];

        let guard = bm.fetch(1, &data, GuardAccess::Write);
        bm.mark_dirty(guard.page_id());
        let flushed = bm.flush(guard.page_id());
        assert!(flushed.is_some());
        bm.unpin(guard.page_id());
    }

    #[test]
    fn test_flush_all() {
        let mut bm = BufferManager::new(4096 * 3);
        let data = [0u8; PAGE_SIZE];

        let g1 = bm.fetch(1, &data, GuardAccess::Write);
        let g2 = bm.fetch(2, &data, GuardAccess::Write);
        bm.mark_dirty(g1.page_id());
        bm.mark_dirty(g2.page_id());
        bm.unpin(g1.page_id());
        bm.unpin(g2.page_id());

        let flushed = bm.flush_all();
        assert_eq!(flushed.len(), 2);
    }
}

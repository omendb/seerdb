//! Page guards: RAII-based access control for buffer frames.
//!
//! Guards ensure that pages are properly pinned/unpinned and provide
//! controlled access to page data.

/// Access level for a page guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardAccess {
    /// Read-only access (shared).
    Read,
    /// Read-write access (exclusive).
    Write,
}

/// RAII guard for a page in the buffer pool.
///
/// The guard holds a frame index and access level. It does not borrow
/// the buffer manager, allowing multiple guards to coexist.
/// Unpinning is handled manually via `BufferManager::unpin`.
pub struct PageGuard {
    /// Index into the buffer pool's frame array.
    frame_index: usize,
    /// Page ID.
    page_id: u64,
    /// Access level.
    access: GuardAccess,
}

impl PageGuard {
    /// Create a new page guard.
    pub(crate) fn new(frame_index: usize, page_id: u64, access: GuardAccess) -> Self {
        Self {
            frame_index,
            page_id,
            access,
        }
    }

    /// Get the frame index.
    pub fn frame_index(&self) -> usize {
        self.frame_index
    }

    /// Get the page ID.
    pub fn page_id(&self) -> u64 {
        self.page_id
    }

    /// Get the access level.
    pub fn access(&self) -> GuardAccess {
        self.access
    }

    /// Whether this guard has write access.
    pub fn is_writable(&self) -> bool {
        self.access == GuardAccess::Write
    }
}

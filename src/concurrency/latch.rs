//! Hybrid latch: supports both optimistic and exclusive locking.
//!
//! A hybrid latch can be in one of three states:
//! - Unlocked: no readers or writers
//! - ReadLocked: one or more readers (shared)
//! - WriteLocked: single writer (exclusive)

use std::sync::atomic::{AtomicU32, Ordering};

/// Latch state constants.
const UNLOCKED: u32 = 0;
const READ_LOCKED: u32 = 1;
const WRITE_LOCKED: u32 = u32::MAX;

/// A hybrid latch supporting optimistic and exclusive locking.
///
/// This is a lightweight synchronization primitive for protecting
/// B-tree nodes. It supports:
/// - Optimistic read: read without locking, validate after
/// - Shared read: multiple concurrent readers
/// - Exclusive write: single writer, blocks all others
pub struct HybridLatch {
    /// State: UNLOCKED, READ_LOCKED (count), or WRITE_LOCKED.
    state: AtomicU32,
}

impl HybridLatch {
    /// Create a new unlocked latch.
    pub fn new() -> Self {
        Self {
            state: AtomicU32::new(UNLOCKED),
        }
    }

    /// Try to acquire a shared (read) lock.
    ///
    /// Returns true if the lock was acquired, false if the latch is write-locked.
    pub fn try_read_lock(&self) -> bool {
        let mut current = self.state.load(Ordering::Acquire);

        loop {
            if current == WRITE_LOCKED {
                return false; // write-locked, can't acquire read lock
            }

            let new_state = if current == UNLOCKED {
                READ_LOCKED
            } else {
                current + 1 // increment reader count
            };

            match self.state.compare_exchange_weak(
                current,
                new_state,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// Release a shared (read) lock.
    pub fn read_unlock(&self) {
        let mut current = self.state.load(Ordering::Acquire);

        loop {
            if current == UNLOCKED || current == WRITE_LOCKED {
                return; // not read-locked, nothing to do
            }

            let new_state = if current == READ_LOCKED {
                UNLOCKED
            } else {
                current - 1
            };

            match self.state.compare_exchange_weak(
                current,
                new_state,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }

    /// Try to acquire an exclusive (write) lock.
    ///
    /// Returns true if the lock was acquired, false if the latch is already locked.
    pub fn try_write_lock(&self) -> bool {
        self.state
            .compare_exchange(UNLOCKED, WRITE_LOCKED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Release an exclusive (write) lock.
    pub fn write_unlock(&self) {
        self.state.store(UNLOCKED, Ordering::Release);
    }

    /// Check if the latch is currently locked (for optimistic validation).
    pub fn is_locked(&self) -> bool {
        self.state.load(Ordering::Acquire) != UNLOCKED
    }

    /// Get the current reader count (for debugging).
    pub fn reader_count(&self) -> u32 {
        let state = self.state.load(Ordering::Acquire);
        if state == WRITE_LOCKED {
            0 // write-locked, no readers
        } else {
            state
        }
    }
}

impl Default for HybridLatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_read_lock() {
        let latch = HybridLatch::new();
        assert!(latch.try_read_lock());
        assert!(latch.is_locked());
        assert_eq!(latch.reader_count(), 1);
        latch.read_unlock();
        assert!(!latch.is_locked());
    }

    #[test]
    fn test_multiple_readers() {
        let latch = HybridLatch::new();
        assert!(latch.try_read_lock());
        assert!(latch.try_read_lock());
        assert_eq!(latch.reader_count(), 2);
        latch.read_unlock();
        assert_eq!(latch.reader_count(), 1);
        latch.read_unlock();
        assert!(!latch.is_locked());
    }

    #[test]
    fn test_write_lock() {
        let latch = HybridLatch::new();
        assert!(latch.try_write_lock());
        assert!(latch.is_locked());
        assert!(!latch.try_read_lock()); // can't read while write-locked
        assert!(!latch.try_write_lock()); // can't double write-lock
        latch.write_unlock();
        assert!(!latch.is_locked());
    }

    #[test]
    fn test_write_blocks_read() {
        let latch = HybridLatch::new();
        assert!(latch.try_write_lock());
        assert!(!latch.try_read_lock());
        latch.write_unlock();
        assert!(latch.try_read_lock());
    }

    #[test]
    fn test_read_blocks_write() {
        let latch = HybridLatch::new();
        assert!(latch.try_read_lock());
        assert!(!latch.try_write_lock());
        latch.read_unlock();
        assert!(latch.try_write_lock());
    }

    #[test]
    fn test_concurrent_reads() {
        let latch = Arc::new(HybridLatch::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let latch = Arc::clone(&latch);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    while !latch.try_read_lock() {
                        thread::yield_now();
                    }
                    // Simulate read work.
                    thread::yield_now();
                    latch.read_unlock();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert!(!latch.is_locked());
    }

    #[test]
    fn test_concurrent_write() {
        let latch = Arc::new(HybridLatch::new());
        let counter = Arc::new(AtomicU32::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let latch = Arc::clone(&latch);
            let counter = Arc::clone(&counter);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    while !latch.try_write_lock() {
                        thread::yield_now();
                    }
                    counter.fetch_add(1, Ordering::Relaxed);
                    latch.write_unlock();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::Relaxed), 1000);
    }
}

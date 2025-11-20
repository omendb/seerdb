use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub type FrameId = usize;

pub trait EvictionPolicy: Send + Sync {
    fn access(&self, frame_id: FrameId);
    fn evict(&self) -> Option<FrameId>;
    fn remove(&self, frame_id: FrameId);
}

/// Clock (Second Chance) replacement policy.
/// Efficient O(1) access and eviction.
pub struct ClockPolicy {
    hand: AtomicUsize,
    capacity: usize,
    reference_bits: Vec<AtomicBool>,
}

impl ClockPolicy {
    pub fn new(capacity: usize) -> Self {
        let mut bits = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            bits.push(AtomicBool::new(false));
        }
        Self {
            hand: AtomicUsize::new(0),
            capacity,
            reference_bits: bits,
        }
    }
}

impl EvictionPolicy for ClockPolicy {
    fn access(&self, frame_id: FrameId) {
        if frame_id < self.capacity {
            self.reference_bits[frame_id].store(true, Ordering::Relaxed);
        }
    }

    fn evict(&self) -> Option<FrameId> {
        // Spin the clock hand at most 2 times around to find a victim.
        // If everyone is referenced, we might cycle forever, so limit loops.
        let start_hand = self.hand.load(Ordering::Relaxed);
        let mut loops = 0;
        
        loop {
            let current_hand = self.hand.fetch_add(1, Ordering::Relaxed) % self.capacity;
            
            // Check reference bit
            if self.reference_bits[current_hand].load(Ordering::Relaxed) {
                // Give second chance: set to 0 and continue
                self.reference_bits[current_hand].store(false, Ordering::Relaxed);
            } else {
                // Found victim (bit was 0)
                return Some(current_hand);
            }

            // Safety valve: if we've scanned 2x capacity, everything is hot.
            // Just return the current hand to force eviction? 
            // Or return None to signal "cache pressure but no victim"?
            // For now, returning None is safer, but a DB usually *needs* a page.
            if current_hand == start_hand {
                loops += 1;
                if loops >= 2 {
                    return None;
                }
            }
        }
    }

    fn remove(&self, frame_id: FrameId) {
        if frame_id < self.capacity {
            self.reference_bits[frame_id].store(false, Ordering::Relaxed);
        }
    }
}

// Alias LruPolicy to ClockPolicy for now, as it's our default
pub type LruPolicy = ClockPolicy;

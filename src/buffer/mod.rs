//! Buffer pool management.
//!
//! The buffer manager pools fixed-size page frames in memory, loading pages
//! on demand and evicting them when the pool is full. Pages are protected
//! by RAII-based guards that control access.

mod manager;
mod frame;
mod guard;

pub use manager::{BufferManager, BufferStats};
pub use frame::Frame;
pub use guard::{PageGuard, GuardAccess};

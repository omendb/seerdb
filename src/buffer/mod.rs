pub mod manager;
pub mod eviction;

pub use manager::{BufferPool, BufferPoolOptions, PageId, FrameRef};
pub use eviction::{EvictionPolicy, LruPolicy};

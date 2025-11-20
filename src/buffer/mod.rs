mod manager;
mod eviction;

pub use manager::{BufferPool, BufferPoolOptions, PageId, FrameRef};
pub use eviction::{EvictionPolicy, LruPolicy};

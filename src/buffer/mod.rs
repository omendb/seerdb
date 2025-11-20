pub mod manager;
pub mod eviction;

pub use manager::{BufferPool, BufferPoolOptions, PageId, FrameRef};
pub use eviction::{EvictionPolicy, LruPolicy};

use thiserror::Error;

/// BufferPool error types
#[derive(Debug, Error)]
pub enum BufferPoolError {
    #[error("Buffer pool is full (all frames pinned or in use)")]
    PoolFull,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

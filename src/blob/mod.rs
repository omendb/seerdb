//! Blob file management for KV separation.
//!
//! Large values (>blob_threshold) are stored in append-only blob files.
//! The B-tree stores blob pointers (file_id, offset, length) instead of
//! the actual values.

mod manager;
mod file;

pub use manager::BlobManager;
pub use file::BlobFile;

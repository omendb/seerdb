//! B-tree data structure with out-of-place writes.
//!
//! Nodes are fixed-size (4KB default) pages containing sorted key-value pairs
//! with prefix compression. Pages are never updated in place — writes create
//! new versions at different locations tracked by the PMT.

pub(crate) mod node;
mod tree;

pub use node::{Node, NodeHeader, PageType, ValueType, Tombstone, BlobPointer, ValueRef, InsertError, SplitError, PAGE_SIZE, BLOB_POINTER_SIZE};
pub use tree::{BTree, LookupResult, RangeScan};

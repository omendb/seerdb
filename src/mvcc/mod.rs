//! Multi-version concurrency control.
//!
//! This module implements MVCC components including the Page Mapping Table (PMT)
//! which tracks the current location of each page in the out-of-place B-tree.

mod pmt;

pub use pmt::{PageMapping, PMT};

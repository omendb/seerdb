//! File management and page allocation.
//!
//! This module handles the on-disk storage format and I/O operations.
//! On Linux, it supports O_DIRECT for bypassing the page cache.

mod device;

pub use device::{Device, DeviceOptions};

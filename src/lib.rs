//! seerdb — High-performance out-of-place B-tree storage engine for NVMe SSDs
//!
//! A storage engine designed from scratch for modern hardware, combining:
//! - **Out-of-place writes** (LeanStore-inspired): pages are never updated in place
//! - **KV separation** (WiscKey-inspired): large values stored separately
//! - **SSD-native design**: FDP/ZNS support for minimal write amplification
//! - **MVCC**: copy-on-write concurrency control
//!
//! # Architecture
//!
//! seerdb uses an out-of-place B-tree where writes create new page versions
//! instead of modifying pages in place. A mapping table tracks page locations,
//! and garbage collection reclaims invalidated pages. Large values are stored
//! separately in an append-only blob log.
//!
//! # References
//!
//! - LeanStore (VLDB 2024, 2026): out-of-place B-tree, SSD-aware buffer management
//! - "How to Write to SSDs" (VLDB 2026): DB-SSD co-optimization, NoWA pattern
//! - WiscKey (FAST 2016): key-value separation for reduced write amplification
//! - Tidehunter (2026): WAL-as-store architecture (reference for I/O patterns)

mod btree;
mod buffer;
mod blob;
mod concurrency;
mod recovery;
mod space;
mod mvcc;

// Public API (to be implemented)
// pub use btree::BTree;
// pub use buffer::BufferManager;

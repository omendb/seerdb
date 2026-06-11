//! Crash recovery via Write-Ahead Logging (WAL).
//!
//! The WAL ensures that all mutations are logged before they are applied.
//! On crash recovery, the WAL is replayed to restore the database to a
//! consistent state.

mod wal;

pub use wal::{WalRecord, WalManager, SyncPolicy};

// seerdb: Research-grade storage engine with learned data structures
// License: Elastic-2.0

pub mod bloom;
pub mod compaction;
pub mod db;
pub mod health;
pub mod memtable;
pub mod metrics;
pub mod sstable;
pub mod vlog;
pub mod wal;

pub use bloom::{BitPackedBloomFilter, BloomFilter, LearnedBloomFilter};
pub use db::{DB, DBOptions};
pub use health::{CheckStatus, HealthCheck, HealthStatus};
pub use memtable::Memtable;
pub use metrics::DBStats;
pub use sstable::{SSTable, SSTableBuilder};
pub use vlog::{ValuePointer, VLog};
pub use wal::{Record, SyncPolicy, WAL};

#[cfg(test)]
mod tests {
    #[test]
    fn basic_test() {
        assert_eq!(2 + 2, 4);
    }
}

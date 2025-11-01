// seerdb: Research-grade storage engine with learned data structures
// License: Elastic-2.0

pub mod bloom;
pub mod memtable;
pub mod wal;

pub use bloom::{BloomFilter, LearnedBloomFilter};
pub use memtable::Memtable;
pub use wal::{Record, WAL};

#[cfg(test)]
mod tests {
    #[test]
    fn basic_test() {
        assert_eq!(2 + 2, 4);
    }
}

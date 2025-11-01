// seerdb: Research-grade storage engine with learned data structures
// License: Elastic-2.0

pub mod bloom;

pub use bloom::{BloomFilter, LearnedBloomFilter};

#[cfg(test)]
mod tests {
    #[test]
    fn basic_test() {
        assert_eq!(2 + 2, 4);
    }
}

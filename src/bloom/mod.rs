// Bloom filter implementation
//
// Uses bit-packed Vec<u64> storage (standard approach, 8x more efficient than naive Vec<bool>)
// Learned bloom filter available but has accuracy issues (48-51% FPR vs 1% target)

mod bitpacked;
mod learned;

#[cfg(test)]
mod traditional; // Naive Vec<bool> implementation for benchmarking only

pub use bitpacked::BloomFilter;
pub use learned::LearnedBloomFilter;

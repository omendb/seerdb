// Bloom filter implementation
//
// Uses bit-packed Vec<u64> storage (standard approach, 8x more efficient than naive Vec<bool>)
// Learned bloom filter available but has accuracy issues (48-51% FPR vs 1% target)

mod bitpacked;
mod learned;
// TODO: Blocked bloom filter - 3x speedup expected (cache-line locality)
//       Research shows promise, needs proper multi-word bit implementation
//       Defer until after RocksDB comparison proves core claims
// mod simd; // Disabled: 18% regression on negative lookups (hot path) - see SIMD_OPPORTUNITIES.md

#[cfg(test)]
mod traditional; // Naive Vec<bool> implementation for benchmarking only

pub use bitpacked::BloomFilter;
pub use learned::LearnedBloomFilter;
// pub use simd::SimdBloomFilter; // Disabled - see SIMD_OPPORTUNITIES.md

// Bloom filter implementations: Traditional, Bit-packed, and Learned
//
// OPTIMIZATION: BitPackedBloomFilter is now the default (8x space savings vs traditional)
// Traditional bloom used Vec<bool> (~1 byte per bit), BitPacked uses Vec<u64> (~0.125 bytes per bit)

mod bitpacked;
mod learned;
mod traditional;

pub use bitpacked::BitPackedBloomFilter;
pub use learned::LearnedBloomFilter;
pub use traditional::BloomFilter as TraditionalBloomFilter;

// Use bit-packed bloom as default for 8x space savings
pub use bitpacked::BitPackedBloomFilter as BloomFilter;

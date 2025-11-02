# SSTable Checksum Design

**Last Updated**: November 1, 2025
**Status**: Design phase
**Priority**: HIGH-2 (production blocker)

---

## Current State

**vLog**: Has CRC32 checksums ✅
**WAL**: Has CRC32 checksums ✅
**SSTables**: NO checksums ❌
**LSM metadata**: NO checksums ❌

---

## Problem

SSTables store critical data but have no corruption detection:
- Bit flips on disk → silent data corruption
- Incomplete writes → corrupt SSTables read as valid
- Hardware errors → undetected bad data
- No way to detect tampering or corruption

**Impact**: Data loss, incorrect query results, database corruption

---

## Design Options

### Option 1: Per-Section Checksums (Recommended)
Checksum each major section separately for granular error detection.

**New Format**:
```
[entries_section]
[entries_checksum: u32]
[index_section]
[index_checksum: u32]
[bloom_section]
[bloom_checksum: u32]
[footer: index_offset, bloom_offset, version]
```

**Pros**:
- Granular error detection (know which section is corrupt)
- Can skip corrupted sections if desired
- Better error messages ("index corrupted" vs "file corrupted")

**Cons**:
- More complex implementation
- Larger file size (+12 bytes for 3 checksums)
- More verification overhead

### Option 2: Single File Checksum
One checksum for entire file content.

**New Format**:
```
[entries_section]
[index_section]
[bloom_section]
[footer: index_offset, bloom_offset, checksum: u32]
```

**Pros**:
- Simple implementation
- Smaller overhead (+4 bytes)
- Fast verification (single CRC32 pass)

**Cons**:
- Less granular error messages
- Can't identify which section is corrupt
- Must re-checksum entire file on write

### Option 3: Per-Entry Checksums (Overkill)
Checksum each key-value entry individually.

**Pros**:
- Very granular error detection
- Can skip individual corrupt entries

**Cons**:
- Large overhead (4 bytes per entry)
- Slow verification (checksum every entry)
- Complex implementation
- Not how other databases do it

---

## Recommendation: Option 1 (Per-Section Checksums)

**Rationale**:
- Production databases need granular error reporting
- 12 extra bytes is negligible vs SSTable size (typically MB+)
- Matches how RocksDB and other engines handle this
- Better debugging: "bloom filter corrupted at offset X"

**Implementation**:
- Use `crc32fast` (already in dependencies, hardware-accelerated)
- Checksum after writing each section
- Verify on read before using data
- Return errors (don't panic) on corruption

---

## Updated SSTable Format

### Write Order:
```
1. Write entries section
2. Calculate entries_checksum (CRC32 of entries bytes)
3. Write entries_checksum

4. Write index section
5. Calculate index_checksum (CRC32 of index bytes)
6. Write index_checksum

7. Write bloom filter section
8. Calculate bloom_checksum (CRC32 of bloom bytes)
9. Write bloom_checksum

10. Write footer:
    - index_offset: u64 (offset to index section)
    - bloom_offset: u64 (offset to bloom section)
    - entries_checksum_offset: u64 (offset to entries checksum)
    - index_checksum_offset: u64 (offset to index checksum)
    - bloom_checksum_offset: u64 (offset to bloom checksum)
    - version: u32 (format version, start with 1)
```

### Read Order:
```
1. Read footer (last 44 bytes: 5*u64 + u32)
2. Check version (must be 1, else InvalidFormat)
3. Read and verify entries_checksum
4. Read and verify index_checksum
5. Read and verify bloom_checksum
6. If all checksums valid, proceed with normal read
7. If any checksum invalid, return Corruption error
```

---

## Implementation Plan

### Phase 1: Add Checksum Writing (No Breaking Change)
- Add checksum calculation to `SSTableBuilder::build()`
- Write checksums to file
- Update footer format with version field
- Existing SSTables without checksums remain readable (version 0)

### Phase 2: Add Checksum Verification
- Add `verify_checksums()` method to SSTable
- Call on `SSTable::open()` (optional flag)
- Return `SSTableError::Corruption` on mismatch
- Log warnings for version 0 (no checksums)

### Phase 3: Make Checksums Mandatory
- Remove support for version 0
- Always verify checksums on read
- Fail loudly on corruption

### Phase 4: Add Corruption Tests
- Test: Flip bits in entries section → detect corruption
- Test: Truncate file → detect corruption
- Test: Modify index → detect corruption
- Test: Modify bloom filter → detect corruption

---

## Error Handling

### New Error Variant:
```rust
#[error("SSTable corrupted: {section} at offset {offset}")]
Corruption {
    section: String, // "entries", "index", or "bloom"
    offset: u64,
    expected: u32,   // Expected checksum
    actual: u32,     // Actual checksum
}
```

### Error Recovery:
- On corruption: Return error to caller
- Caller can decide: retry, skip SSTable, or abort
- Log corruption events for debugging
- Future: Add SSTable rebuilding from WAL

---

## Backward Compatibility

**Version Field**:
- Version 0: No checksums (legacy, current format)
- Version 1: Per-section checksums (new format)
- Future versions: Can add compression, encryption, etc.

**Migration Path**:
1. Phase 1: Write version 1, read version 0 or 1
2. Phase 2: Recompact old SSTables to version 1
3. Phase 3: Drop support for version 0

**Timeline**:
- Week 1: Implement version 1 writing
- Week 2: Implement verification
- Week 3: Background recompaction of version 0 files
- Week 4: Drop version 0 support

---

## Performance Impact

**Write Performance**:
- CRC32 is hardware-accelerated (SSE 4.2 on x86, ARMv8 CRC on ARM)
- ~1-2 GB/s throughput (negligible vs disk I/O)
- Adds ~5-10% overhead to SSTable build

**Read Performance**:
- Verification on open: one-time cost
- Lazy verification: only when reading sections
- Cached SSTables: verify once, use many times
- Minimal impact (<1% on queries)

**Storage Overhead**:
- 12 bytes per SSTable (3 checksums)
- Footer grows from 16 to 44 bytes
- Total: +28 bytes per SSTable
- Negligible vs typical SSTable size (1MB+)

---

## Testing Strategy

### Unit Tests:
- Test checksum calculation correctness
- Test version field serialization
- Test backward compatibility (version 0 → version 1)

### Integration Tests:
- Test full SSTable write with checksums
- Test SSTable read with verification
- Test corruption detection (flip bits)
- Test graceful degradation (skip corrupt SSTables)

### Corruption Tests:
- Flip random bits in entries → detect
- Flip random bits in index → detect
- Flip random bits in bloom → detect
- Truncate file → detect
- Overwrite with zeros → detect

### Performance Tests:
- Benchmark write overhead (with/without checksums)
- Benchmark read overhead (with/without verification)
- Ensure <10% regression

---

## Future Enhancements

**Version 2**: Add compression
- Per-section compression (LZ4, Zstd)
- Checksum compressed data
- Better space efficiency

**Version 3**: Add encryption
- Per-section encryption (AES-GCM)
- Authenticated encryption (built-in integrity)
- Secure storage

**Version 4**: Add block-based format
- RocksDB-style blocks
- Block-level checksums
- Block cache support

---

*Last Updated: November 1, 2025*
*Priority: HIGH-2 (production blocker)*
*Timeline: 1-2 weeks implementation*

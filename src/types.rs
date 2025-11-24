use bytes::{BufMut, Bytes, BytesMut};
use std::cmp::Ordering;

/// Value type for Internal Key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ValueType {
    /// A live deletion (tombstone)
    Deletion = 0x00,
    /// A standard value
    Value = 0x01,
    /// A merge operand
    Merge = 0x02,
    /// Log data (not stored in memtable usually)
    Log = 0x03,
}

impl ValueType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(ValueType::Deletion),
            0x01 => Some(ValueType::Value),
            0x02 => Some(ValueType::Merge),
            0x03 => Some(ValueType::Log),
            _ => None,
        }
    }
}

/// Internal Key used for MVCC (Multi-Version Concurrency Control)
///
/// Format: [ User Key ] [ 8 bytes: (SeqNum << 8) | ValueType ]
///
/// Sorting:
/// 1. User Key (Ascending)
/// 2. Sequence Number (Descending) - Latest version comes first
#[derive(Debug, Clone, Eq)]
pub struct InternalKey {
    pub user_key: Bytes,
    pub seq: u64,
    pub kind: ValueType,
}

impl InternalKey {
    pub fn new(user_key: Bytes, seq: u64, kind: ValueType) -> Self {
        Self {
            user_key,
            seq,
            kind,
        }
    }

    /// Create a search key that will match the latest version of a user key
    /// (Sequence number = MAX)
    pub fn for_lookup(user_key: Bytes) -> Self {
        Self {
            user_key,
            seq: u64::MAX,
            kind: ValueType::Value, // Kind doesn't matter for lookup start
        }
    }

    /// Encode the InternalKey into bytes for storage
    /// Uses big-endian encoding for the sequence number wrapper to preserve sort order
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.user_key.len() + 8);
        buf.extend_from_slice(&self.user_key);
        
        // Pack Seq + Type into 64 bits
        // We use 56 bits for Seq, 8 bits for Type.
        // This limits Seq to ~72 quadrillion (plenty).
        // To sort Descending, we store (MAX - Seq).
        let packed = (self.seq << 8) | (self.kind as u64);
        let inverted = !packed; // Bitwise NOT reverses the order
        
        buf.put_u64(inverted);
        buf.freeze()
    }

    /// Decode an InternalKey from bytes
    pub fn decode(bytes: Bytes) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }

        let split_idx = bytes.len() - 8;
        let user_key = bytes.slice(..split_idx);
        let trailer = bytes.slice(split_idx..);
        
        let inverted = u64::from_be_bytes(trailer.as_ref().try_into().ok()?);
        let packed = !inverted;
        
        let kind_u8 = (packed & 0xFF) as u8;
        let seq = packed >> 8;
        
        let kind = ValueType::from_u8(kind_u8)?;

        Some(Self {
            user_key,
            seq,
            kind,
        })
    }

    /// Extract just the user key from an encoded buffer (zero copy)
    /// If the key is shorter than 9 bytes (min: 1 byte user key + 8 byte trailer),
    /// returns the key unchanged (assumes it's a plain user key, not an InternalKey).
    pub fn extract_user_key(bytes: &Bytes) -> Bytes {
        if bytes.len() <= 8 {
            // Too short to be an InternalKey - treat as plain user key
            return bytes.clone();
        }
        bytes.slice(..bytes.len() - 8)
    }
}

impl PartialEq for InternalKey {
    fn eq(&self, other: &Self) -> bool {
        self.user_key == other.user_key && self.seq == other.seq && self.kind == other.kind
    }
}

impl PartialOrd for InternalKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InternalKey {
    fn cmp(&self, other: &Self) -> Ordering {
        // 1. Compare User Key (Ascending)
        match self.user_key.cmp(&other.user_key) {
            Ordering::Equal => {
                // 2. Compare Sequence Number (Descending)
                // Higher sequence number should come FIRST (Less)
                other.seq.cmp(&self.seq)
            }
            ord => ord,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_internal_key_encoding() {
        let key = InternalKey::new(Bytes::from("key"), 100, ValueType::Value);
        let encoded = key.encode();
        let decoded = InternalKey::decode(encoded).unwrap();
        
        assert_eq!(decoded.user_key, Bytes::from("key"));
        assert_eq!(decoded.seq, 100);
        assert_eq!(decoded.kind, ValueType::Value);
    }

    #[test]
    fn test_internal_key_sorting() {
        // key v2 (latest)
        let k2 = InternalKey::new(Bytes::from("abc"), 200, ValueType::Value);
        // key v1 (older)
        let k1 = InternalKey::new(Bytes::from("abc"), 100, ValueType::Value);
        // key b (different key)
        let kb = InternalKey::new(Bytes::from("abd"), 100, ValueType::Value);

        // k2 should be "Less" than k1 because it's newer (descending sort)
        assert_eq!(k2.cmp(&k1), Ordering::Less);
        
        // k1 should be "Less" than kb because "abc" < "abd"
        assert_eq!(k1.cmp(&kb), Ordering::Less);
    }

    #[test]
    fn test_encoded_byte_sorting() {
        let k2 = InternalKey::new(Bytes::from("abc"), 200, ValueType::Value);
        let k1 = InternalKey::new(Bytes::from("abc"), 100, ValueType::Value);
        
        let e2 = k2.encode();
        let e1 = k1.encode();
        
        // Raw byte comparison should match logical comparison
        assert!(e2 < e1);
    }
}

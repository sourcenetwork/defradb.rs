//! A CID's binary and text forms, written without the allocations `cid`'s own
//! `to_bytes` and `to_string` make (a growing vector for the bytes; for the
//! text, the bytes again, the base32 string, its multibase prefix shifted in
//! front, and the formatter's string around it).
//!
//! The bytes are `cid`'s: the version and codec varints (CIDv1), then the
//! multihash's code and size varints and its digest. The text of a CIDv1 is
//! its multibase base32 form (`b`, then RFC 4648 lowercase, unpadded), built
//! on the stack; a CIDv0 (base58btc) is rare enough to keep `cid`'s own text.

use std::cmp::Ordering;
use std::fmt;
use std::ops::Deref;

use cid::{Cid, Version};

/// The most bytes a CIDv1 takes: version, codec and hash code varints (one,
/// ten and ten bytes at most), the size byte, and a digest of the 64 bytes a
/// `Cid` holds.
const INLINE_BYTES: usize = 1 + 10 + 10 + 1 + 64;
/// `b` and the base32 digits of [`INLINE_BYTES`] bytes.
const INLINE_TEXT: usize = 1 + (INLINE_BYTES * 8).div_ceil(5);

const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Appends `cid`'s bytes, exactly what `Cid::to_bytes` returns, to `out`.
pub fn push_cid_bytes(out: &mut Vec<u8>, cid: &Cid) {
    out.reserve(cid.encoded_len());
    write_cid(cid, |bytes| out.extend_from_slice(bytes));
}

/// `cid`'s bytes in a vector of exactly their length.
pub fn cid_bytes(cid: &Cid) -> Vec<u8> {
    let mut out = Vec::with_capacity(cid.encoded_len());
    push_cid_bytes(&mut out, cid);
    out
}

/// Hands `cid`'s bytes to `put` in order: the version and codec varints of a
/// CIDv1, then the multihash's code and size varints and its digest.
fn write_cid<F>(cid: &Cid, mut put: F)
where
    F: FnMut(&[u8]),
{
    let hash = cid.hash();
    if cid.version() == Version::V1 {
        put(varint(u64::from(Version::V1)).as_slice());
        put(varint(cid.codec()).as_slice());
    }
    put(varint(hash.code()).as_slice());
    put(varint(u64::from(hash.size())).as_slice());
    put(hash.digest());
}

/// Unsigned LEB128, the `unsigned-varint` encoding `cid` writes.
fn varint(mut value: u64) -> Varint {
    let mut varint = Varint {
        bytes: [0; 10],
        len: 0,
    };
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            varint.bytes[varint.len] = byte;
            varint.len += 1;
            return varint;
        }
        varint.bytes[varint.len] = byte | 0x80;
        varint.len += 1;
    }
}

/// A varint of up to the ten bytes a `u64` takes.
struct Varint {
    bytes: [u8; 10],
    len: usize,
}

impl Varint {
    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// A CID's text, as `Display` writes it, held without a heap allocation
/// when it is a CIDv1 that fits. Orders as its text.
#[derive(Clone)]
pub struct CidText(Repr);

#[derive(Clone)]
enum Repr {
    /// `b` and base32 digits only: ASCII.
    Inline {
        text: [u8; INLINE_TEXT],
        len: u8,
    },
    Owned(String),
}

impl CidText {
    pub fn new(cid: &Cid) -> Self {
        let Some((bytes, len)) = inline_bytes(cid) else {
            return Self(Repr::Owned(cid.to_string()));
        };
        let mut text = [0u8; INLINE_TEXT];
        text[0] = b'b';
        let mut at = 1;
        let mut acc: u16 = 0;
        let mut bits = 0;
        for &byte in &bytes[..len] {
            acc = (acc << 8) | u16::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                text[at] = BASE32[usize::from((acc >> bits) & 31)];
                at += 1;
            }
            acc &= (1 << bits) - 1;
        }
        if bits > 0 {
            text[at] = BASE32[usize::from((acc << (5 - bits)) & 31)];
            at += 1;
        }
        Self(Repr::Inline {
            text,
            len: at as u8,
        })
    }

    pub fn as_str(&self) -> &str {
        match &self.0 {
            // SAFETY: an inline text holds only `b` and base32 digits, all
            // ASCII, so it is valid UTF-8.
            Repr::Inline { text, len } => unsafe {
                std::str::from_utf8_unchecked(&text[..usize::from(*len)])
            },
            Repr::Owned(text) => text,
        }
    }
}

/// A CIDv1's bytes on the stack, or `None` for a CIDv0 or one too long.
fn inline_bytes(cid: &Cid) -> Option<([u8; INLINE_BYTES], usize)> {
    if cid.version() != Version::V1 || cid.encoded_len() > INLINE_BYTES {
        return None;
    }
    let mut bytes = [0u8; INLINE_BYTES];
    let mut len = 0;
    let mut fits = true;
    write_cid(cid, |piece| match bytes.get_mut(len..len + piece.len()) {
        Some(slot) => {
            slot.copy_from_slice(piece);
            len += piece.len();
        }
        None => fits = false,
    });
    fits.then_some((bytes, len))
}

impl Deref for CidText {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for CidText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for CidText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl PartialEq for CidText {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for CidText {}

impl PartialOrd for CidText {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CidText {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

#[cfg(test)]
mod tests {
    use multihash::Multihash;
    use proptest::prelude::*;

    use super::*;

    /// CIDs of both versions, codecs and hash codes of every varint width,
    /// and digests from empty to the 64 bytes a `Cid` holds.
    fn cid_strategy() -> impl Strategy<Value = Cid> {
        prop_oneof![
            (
                any::<u64>(),
                any::<u64>(),
                prop::collection::vec(any::<u8>(), 0..=64)
            )
                .prop_map(|(codec, code, digest)| {
                    let hash = Multihash::<64>::wrap(code, &digest).expect("a digest that fits");
                    Cid::new_v1(codec, hash)
                }),
            (0..0x200u64, prop::collection::vec(any::<u8>(), 0..=64)).prop_map(
                |(codec, digest)| {
                    let hash = Multihash::<64>::wrap(0x12, &digest).expect("a digest that fits");
                    Cid::new_v1(codec, hash)
                }
            ),
            prop::array::uniform32(any::<u8>()).prop_map(|digest| {
                let hash = Multihash::<64>::wrap(0x12, &digest).expect("a SHA2-256 digest");
                Cid::new_v0(hash).expect("a CIDv0")
            }),
        ]
    }

    proptest! {
        #[test]
        fn bytes_are_cids_own(cid in cid_strategy()) {
            let bytes = cid_bytes(&cid);
            prop_assert_eq!(&bytes, &cid.to_bytes());
            prop_assert_eq!(bytes.len(), bytes.capacity());
            let mut appended = b"prefix".to_vec();
            push_cid_bytes(&mut appended, &cid);
            prop_assert_eq!(&appended[6..], &bytes[..]);
        }

        #[test]
        fn text_is_cids_own(cid in cid_strategy()) {
            let text = CidText::new(&cid);
            prop_assert_eq!(text.as_str(), cid.to_string());
            prop_assert_eq!(text.to_string(), cid.to_string());
        }

        #[test]
        fn orders_as_the_text(a in cid_strategy(), b in cid_strategy()) {
            prop_assert_eq!(
                CidText::new(&a).cmp(&CidText::new(&b)),
                a.to_string().cmp(&b.to_string())
            );
        }
    }

    #[test]
    fn a_sha2_cidv1_stays_on_the_stack() {
        let hash = Multihash::<64>::wrap(0x12, &[7; 32]).expect("a SHA2-256 digest");
        let cid = Cid::new_v1(0x71, hash);
        assert!(matches!(CidText::new(&cid).0, Repr::Inline { .. }));
    }
}

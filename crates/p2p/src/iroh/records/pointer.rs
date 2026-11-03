//! The pointer a node publishes in its pkarr user data: which record is
//! current, small enough for the slot. Proof:
//! `proofs/lean/ServiceRecord/Pointer.lean`.

const PREFIX: &str = "dsr1:";
/// `dsr1:` + 16 hex + `:` + 16 hex.
const LEN: usize = PREFIX.len() + 16 + 1 + 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pointer {
    pub seq: u64,
    /// The first 8 bytes of the signed record's hash.
    pub hash: u64,
}

impl Pointer {
    pub fn print(&self) -> String {
        format!("{PREFIX}{:016x}:{:016x}", self.seq, self.hash)
    }

    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != LEN || !text.starts_with(PREFIX) {
            return None;
        }
        let (seq, hash) = text[PREFIX.len()..].split_once(':')?;
        Some(Self {
            seq: hex16(seq)?,
            hash: hex16(hash)?,
        })
    }
}

/// Exactly 16 lowercase hex digits, as `print` writes them.
fn hex16(digits: &str) -> Option<u64> {
    let lowercase_hex = digits
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if digits.len() != 16 || !lowercase_hex {
        return None;
    }
    u64::from_str_radix(digits, 16).ok()
}

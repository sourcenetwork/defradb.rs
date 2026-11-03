//! The name of a custom CRDT type, as a field definition declares it.

use std::fmt;

use crate::SchemaError;

/// The names the native types answer to, in the SDL and in configs. A custom
/// type cannot take one, so a name never means two types.
const NATIVE_NAMES: &[&str] = &[
    "none",
    "lww",
    "lwwregister",
    "lww_register",
    "object",
    "composite",
    "pncounter",
    "pn_counter",
    "counter",
    "pcounter",
    "p_counter",
    "text",
    "json",
];

/// The name of a custom CRDT type: 1 to 31 bytes of lowercase ASCII letters,
/// digits and `_`, starting with a letter, and none of the native types'
/// names. Held inline, so [`CType`](crate::CType) stays `Copy`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CrdtName {
    len: u8,
    bytes: [u8; CrdtName::MAX_LEN],
}

impl CrdtName {
    pub const MAX_LEN: usize = 31;

    pub fn new(name: &str) -> Result<Self, SchemaError> {
        let invalid = |reason| SchemaError::InvalidCrdtName {
            name: name.to_owned(),
            reason,
        };
        if name.is_empty() || name.len() > Self::MAX_LEN {
            return Err(invalid("a CRDT name is 1 to 31 bytes long"));
        }
        if !name.starts_with(|c: char| c.is_ascii_lowercase())
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(invalid(
                "a CRDT name is lowercase letters, digits and '_', starting with a letter",
            ));
        }
        if NATIVE_NAMES.contains(&name) {
            return Err(invalid("the name is a native CRDT type's"));
        }
        let mut bytes = [0; Self::MAX_LEN];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        Ok(Self {
            len: name.len() as u8,
            bytes,
        })
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("a CrdtName holds only the ASCII it was checked for")
    }
}

impl fmt::Display for CrdtName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for CrdtName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_reads_back_as_itself() {
        for name in ["maxreg", "a", "or_set2", &"x".repeat(CrdtName::MAX_LEN)] {
            assert_eq!(CrdtName::new(name).unwrap().as_str(), name);
        }
    }

    #[test]
    fn malformed_and_native_names_are_refused() {
        for name in [
            "",
            "MaxReg",
            "2set",
            "_x",
            "max-reg",
            "é",
            &"x".repeat(CrdtName::MAX_LEN + 1),
            "lww",
            "pncounter",
            "text",
            "json",
            "none",
        ] {
            assert!(CrdtName::new(name).is_err(), "{name:?}");
        }
    }
}

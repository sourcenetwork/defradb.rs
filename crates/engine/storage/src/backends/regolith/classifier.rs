//! The store's key classes, declared to regolith's commit validator.
//!
//! The class of every family is declared once, in `families`: blocks are
//! content addressed, the arrival journal is a log, and every other key is
//! ordinary. A head set is one ordinary key: a merge into it is blind at every
//! class, and it is read with `get_parts(key, &[])`, which validates nothing.
//! A key a family does not recognise, or one cut short or malformed, stays
//! ordinary.

use regolith::{KeyClass, KeyClassifier};

use super::families::{self, Class};

pub(super) struct DefraKeyClassifier;

impl KeyClassifier for DefraKeyClassifier {
    fn classify(&self, key: &[u8]) -> KeyClass {
        let Some(family) = families::family_of(key) else {
            return KeyClass::Ordinary;
        };
        match family.class() {
            Class::Ordinary => KeyClass::Ordinary,
            // CID parsing uses fixed-size multihash storage and allocates
            // nothing.
            Class::ContentAddressed if cid::Cid::try_from(family.rest(key)).is_ok() => {
                KeyClass::ContentAddressed
            }
            Class::ContentAddressed => KeyClass::Ordinary,
            Class::Log => KeyClass::Log,
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

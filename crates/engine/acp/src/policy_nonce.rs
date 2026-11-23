//! The nonce a policy creator picks, which vera.rs makes part of the
//! policy's ID together with the definition and the creator.
//!
//! Written as 64 hexadecimal characters, the high half of each byte first;
//! read in either case. Modeled by proofs/lean/VeraPolicyNonce.lean.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Bytes in a nonce.
pub const POLICY_NONCE_BYTES: usize = 32;

/// 32 bytes a policy's creator picks so that one creator can create the
/// same definition more than once, each time under its own ID, and know
/// the ID before the policy exists. A create sent again with the same
/// nonce names the same policy and makes no second one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PolicyNonce(pub [u8; POLICY_NONCE_BYTES]);

/// Text that is not a nonce.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a policy nonce is {} hexadecimal characters", POLICY_NONCE_BYTES * 2)]
pub struct PolicyNonceError;

impl fmt::Display for PolicyNonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

impl FromStr for PolicyNonce {
    type Err = PolicyNonceError;

    fn from_str(text: &str) -> Result<Self, PolicyNonceError> {
        let mut bytes = [0; POLICY_NONCE_BYTES];
        hex::decode_to_slice(text, &mut bytes).map_err(|_| PolicyNonceError)?;
        Ok(Self(bytes))
    }
}

impl Serialize for PolicyNonce {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PolicyNonce {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

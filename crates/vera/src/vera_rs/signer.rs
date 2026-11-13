//! Who signs the delegations an actor gives this node's vera.rs worker.
//!
//! The provider needs three things from an actor's key: the DID it names, the
//! algorithm it signs with, and a signature. It never needs the key itself,
//! so the key may be one this process cannot read. The provider builds every
//! token (`bearer`) and checks each signature against the DID's key before
//! the token leaves the node (`admission`).

use std::fmt;

use defra_core::thread_bounds::{MaybeBoxFuture, MaybeSendSync};

pub(super) mod admission;
mod der;
mod ed25519;
mod p256;
mod secp256k1;

pub use der::signature_from_der;
pub use ed25519::Ed25519Signer;
pub use p256::P256Signer;
pub use secp256k1::Secp256k1Signer;

/// The JWS algorithm of a delegation token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DelegationAlgorithm {
    /// ECDSA over secp256k1 with SHA-256.
    Es256k,
    /// Ed25519.
    EdDsa,
    /// ECDSA over P-256 with SHA-256.
    Es256,
}

impl DelegationAlgorithm {
    /// The `alg` value of the token header.
    pub fn name(self) -> &'static str {
        match self {
            Self::Es256k => "ES256K",
            Self::EdDsa => "EdDSA",
            Self::Es256 => "ES256",
        }
    }
}

impl fmt::Display for DelegationAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Signs delegation tokens for one actor.
pub trait DelegationSigner: MaybeSendSync + 'static {
    /// The actor's `did:key`, of the key type the algorithm signs with. For
    /// a secp256k1 or P-256 key the provider names the actor by the
    /// uncompressed spelling, whichever one this returns.
    fn did(&self) -> &str;

    /// The algorithm [`sign`](Self::sign) signs with.
    fn algorithm(&self) -> DelegationAlgorithm;

    /// Signs `signing_input`, a token's `header.payload`, and returns the JWS
    /// signature. For ES256K and ES256 that is the 64 bytes `r || s`, with
    /// `s` in the lower half of the curve order, over the SHA-256 digest of
    /// `signing_input`; [`signature_from_der`] gives it from a DER signature.
    /// For EdDSA it is the 64-byte Ed25519 signature of `signing_input`.
    fn sign<'a>(
        &'a self,
        signing_input: &'a [u8],
    ) -> MaybeBoxFuture<'a, Result<Vec<u8>, SignerError>>;
}

/// Why a signer could not be used for a delegation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SignerError {
    /// The signer did not produce a signature.
    #[error("signing failed: {0}")]
    Failed(String),

    /// The bytes are not a private key of the curve.
    #[error(transparent)]
    Key(#[from] k256::ecdsa::Error),

    #[error("an {algorithm} private key is 32 bytes, or 64 with its public key, not {length}")]
    KeyLength {
        algorithm: DelegationAlgorithm,
        length: usize,
    },

    #[error("the signature is not an {algorithm} signature in ASN.1 DER")]
    Der { algorithm: DelegationAlgorithm },

    #[error("{did} does not name a key an {algorithm} delegation is signed with: {reason}")]
    Did {
        did: String,
        algorithm: DelegationAlgorithm,
        reason: String,
    },

    #[error(
        "the signer for {did} returned a {length}-byte signature; an {algorithm} signature is \
         64 bytes"
    )]
    Length {
        did: String,
        algorithm: DelegationAlgorithm,
        length: usize,
    },

    #[error(
        "the signer for {did} returned a signature whose r or s is not a scalar of the \
         curve order"
    )]
    Scalar { did: String },

    #[error(
        "the signer for {did} returned a signature with s in the upper half of the curve \
         order; vera.rs accepts only the lower half"
    )]
    HighS { did: String },

    #[error("the signer for {did} returned a signature that the key of {did} does not verify")]
    Unverified { did: String },
}

#[cfg(test)]
mod key_type_tests;
#[cfg(test)]
pub(super) mod tests;

//! Signatures from a signer that answers in ASN.1 DER.

use super::{DelegationAlgorithm, SignerError};

/// The JWS signature for an ECDSA signature in ASN.1 DER, as a hardware key
/// store returns it: `r || s` with `s` moved to the lower half of the curve
/// order, which is the only form vera.rs accepts. A [`DelegationSigner`]
/// over such a store returns this from `sign`.
///
/// [`DelegationSigner`]: super::DelegationSigner
pub fn signature_from_der(
    algorithm: DelegationAlgorithm,
    der: &[u8],
) -> Result<Vec<u8>, SignerError> {
    let malformed = || SignerError::Der { algorithm };
    match algorithm {
        DelegationAlgorithm::Es256k => k256::ecdsa::Signature::from_der(der)
            .map(|signature| signature.normalize_s().to_bytes().to_vec())
            .map_err(|_| malformed()),
        DelegationAlgorithm::Es256 => p256::ecdsa::Signature::from_der(der)
            .map(|signature| signature.normalize_s().to_bytes().to_vec())
            .map_err(|_| malformed()),
        // An Ed25519 signature has no DER form.
        DelegationAlgorithm::EdDsa => Err(malformed()),
    }
}

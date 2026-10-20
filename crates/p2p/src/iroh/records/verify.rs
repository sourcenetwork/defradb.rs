//! Signing a record, and the checks a received record must pass before a
//! node keeps it. Proof: `proofs/lean/ServiceRecord/Validation.lean`.

use iroh::{PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};

use super::{ServiceRecord, MAX_NAME_BYTES, MAX_RECORD_BYTES, MAX_SERVICES, MAX_TTL_SECS};

/// Keeps record signatures apart from anything else the endpoint key signs.
const DOMAIN: &[u8] = b"defra-service-record/1";

/// A record as it travels: the encoded body and the signer's signature
/// over it. The body stays encoded so a node can serve exactly what it got.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedRecord {
    pub body: Vec<u8>,
    pub signature: Vec<u8>,
}

impl SignedRecord {
    /// The first 8 bytes of the record's hash, as its pointer names it.
    pub fn hash(&self) -> u64 {
        let digest = blake3::hash(&postcard::to_allocvec(self).expect("a signed record always encodes"));
        u64::from_be_bytes(digest.as_bytes()[..8].try_into().expect("8 bytes"))
    }
}

/// Why a received record was refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    #[error("record body of {0} bytes is over the limit")]
    TooLarge(usize),
    #[error("record body does not decode")]
    Malformed,
    #[error("record signature does not verify")]
    BadSignature,
    #[error("record has expired")]
    Expired,
    #[error("record expires further ahead than allowed")]
    TooLongLived,
    #[error("record lists {0} services, over the limit")]
    TooManyServices(usize),
    #[error("record carries a name over the limit")]
    NameTooLong,
}

/// Encodes and signs `record` with the node's endpoint key.
pub fn sign(key: &SecretKey, record: &ServiceRecord) -> SignedRecord {
    let body = postcard::to_allocvec(record).expect("a service record always encodes");
    let signature = key.sign(&signed_bytes(&body)).to_bytes().to_vec();
    SignedRecord { body, signature }
}

/// Checks a received record at `now` (unix seconds) and decodes it.
pub fn verify(signed: &SignedRecord, now: u64) -> Result<ServiceRecord, Rejection> {
    if signed.body.len() > MAX_RECORD_BYTES {
        return Err(Rejection::TooLarge(signed.body.len()));
    }
    let record: ServiceRecord =
        postcard::from_bytes(&signed.body).map_err(|_| Rejection::Malformed)?;
    let signature: [u8; Signature::LENGTH] = signed
        .signature
        .as_slice()
        .try_into()
        .map_err(|_| Rejection::BadSignature)?;
    PublicKey::from_bytes(&record.node)
        .map_err(|_| Rejection::BadSignature)?
        .verify(&signed_bytes(&signed.body), &Signature::from_bytes(&signature))
        .map_err(|_| Rejection::BadSignature)?;
    if now >= record.expires_at {
        return Err(Rejection::Expired);
    }
    if record.expires_at > now.saturating_add(MAX_TTL_SECS) {
        return Err(Rejection::TooLongLived);
    }
    if record.services.len() > MAX_SERVICES {
        return Err(Rejection::TooManyServices(record.services.len()));
    }
    let name_too_long = record
        .services
        .iter()
        .filter_map(|service| service.name())
        .any(|name| name.len() > MAX_NAME_BYTES);
    if name_too_long {
        return Err(Rejection::NameTooLong);
    }
    Ok(record)
}

fn signed_bytes(body: &[u8]) -> Vec<u8> {
    [DOMAIN, body].concat()
}

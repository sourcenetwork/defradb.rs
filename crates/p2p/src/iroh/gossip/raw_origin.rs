//! The signed origin of a raw-topic gossip message on iroh.
//!
//! iroh-gossip names only the neighbour that delivered a message and never
//! signs its content, so a relayed pubsub_rpc request (a KMS key request
//! among them) would be answered and authorized as the relay. A publisher
//! therefore seals each raw payload with its endpoint key:
//!
//! `MAGIC (6) | origin public key (32) | signature (64) | payload`
//!
//! The signature covers the topic and the payload, so a sealed request cannot
//! be replayed onto another topic. `MAGIC` opens with `0xFF`, a CBOR break
//! byte that no raw payload (bare CBOR) starts with, so an unsealed payload
//! from a node that predates sealing is told apart from a sealed one.
//!
//! Proved to open back to the sealed origin and payload, and to pass an
//! unsealed payload through untouched, in `proofs/lean/GossipFrame/RawOrigin.lean`.

use iroh::{EndpointId, SecretKey, Signature};

use crate::error::{Error, Result};

const MAGIC: [u8; 6] = [0xFF, b'D', b'R', b'A', b'W', 1];
const KEY_LEN: usize = 32;
const SIGNATURE_LEN: usize = 64;
const HEADER_LEN: usize = MAGIC.len() + KEY_LEN + SIGNATURE_LEN;
const DOMAIN: &[u8] = b"defra/iroh-raw-origin/v1\0";

/// A raw payload as it came off the wire.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Opened<'a> {
    /// Sealed by `origin`, whose signature over the topic and payload holds.
    Signed {
        origin: EndpointId,
        payload: &'a [u8],
    },
    /// Not sealed: sent by a node that predates sealing.
    Unsigned(&'a [u8]),
}

fn signing_bytes(topic: &str, payload: &[u8]) -> Result<Vec<u8>> {
    let topic_len = u32::try_from(topic.len()).map_err(|e| Error::Codec(e.to_string()))?;
    let mut bytes = Vec::with_capacity(DOMAIN.len() + 4 + topic.len() + payload.len());
    bytes.extend_from_slice(DOMAIN);
    bytes.extend_from_slice(&topic_len.to_be_bytes());
    bytes.extend_from_slice(topic.as_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

/// Seal `payload` for `topic` with this node's endpoint key.
pub(crate) fn seal(key: &SecretKey, topic: &str, payload: &[u8]) -> Result<Vec<u8>> {
    let signature = key.sign(&signing_bytes(topic, payload)?);
    let mut sealed = Vec::with_capacity(HEADER_LEN + payload.len());
    sealed.extend_from_slice(&MAGIC);
    sealed.extend_from_slice(key.public().as_bytes());
    sealed.extend_from_slice(&signature.to_bytes());
    sealed.extend_from_slice(payload);
    Ok(sealed)
}

/// Open a raw payload received on `topic`. A sealed payload whose key or
/// signature does not check out is refused, never passed on as unsigned.
pub(super) fn open<'a>(topic: &str, bytes: &'a [u8]) -> Result<Opened<'a>> {
    let Some(rest) = bytes.strip_prefix(&MAGIC) else {
        return Ok(Opened::Unsigned(bytes));
    };
    if rest.len() < KEY_LEN + SIGNATURE_LEN {
        return Err(Error::InvalidSignature);
    }
    let (key, rest) = rest.split_at(KEY_LEN);
    let (signature, payload) = rest.split_at(SIGNATURE_LEN);
    let origin = EndpointId::try_from(key).map_err(|e| Error::InvalidPeerId(e.to_string()))?;
    let signature = Signature::try_from(signature).map_err(|_| Error::InvalidSignature)?;
    origin
        .verify(&signing_bytes(topic, payload)?, &signature)
        .map_err(|_| Error::InvalidSignature)?;
    Ok(Opened::Signed { origin, payload })
}

#[cfg(test)]
#[path = "../../../tests/iroh/gossip_raw_origin.rs"]
mod tests;

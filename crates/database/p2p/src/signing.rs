//! Message signing and verification for DefraDB P2P protocol.
//!
//! This module provides functions for signing outgoing messages and
//! verifying incoming messages, ensuring wire compatibility with the
//! Go implementation.
//!
//! # Wire Compatibility
//!
//! The signing algorithm matches Go's `signAndSetMetaData`:
//! 1. Generate UUID v4 for message_id (if empty)
//! 2. Set version to MESSAGE_VERSION
//! 3. Set sender_id to peer ID string
//! 4. Set pubkey to protobuf-encoded public key
//! 5. CBOR serialize with signature=None
//! 6. Sign the serialized bytes
//! 7. Set signature field
//!
//! Verification matches Go's `verifyMessage`:
//! 1. Decode public key from message
//! 2. Verify peer ID matches public key
//! 3. Clear signature, serialize, verify signature

use serde::Serialize;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::message::Message;
use crate::protocol::MESSAGE_VERSION;

/// Sign a message using a `P2PTransport` for identity and signing.
///
/// This is the transport-agnostic equivalent of `sign_message`. It uses
/// `transport.sign()`, `transport.local_public_key_proto()`, and
/// `transport.local_peer_id()` instead of requiring a raw `Keypair`.
///
/// The serialization format is identical to `sign_message`, maintaining
/// Go wire compatibility.
pub fn sign_with_transport<M, T>(transport: &T, msg: &mut M) -> Result<()>
where
    M: Message + Serialize,
    T: crate::transport::P2PTransport,
{
    if msg.message_id().is_empty() {
        msg.set_message_id(Uuid::new_v4().to_string());
    }

    msg.set_version(MESSAGE_VERSION.to_string());
    msg.set_sender_id(transport.local_peer_id().to_string());
    msg.set_pubkey(transport.local_public_key_proto().to_vec());
    msg.set_signature(None);

    let bytes =
        defra_core::cbor::to_vec(&msg).map_err(|e| Error::CborSerialization(e.to_string()))?;

    let signature = transport.sign(&bytes)?;
    msg.set_signature(Some(signature));

    Ok(())
}

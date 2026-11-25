//! The request/response primitive the KMS and searchable-encryption topics
//! run on, in the shape of Go's
//! [`sourcenetwork/go-libp2p-pubsub-rpc`](https://github.com/sourcenetwork/go-libp2p-pubsub-rpc):
//! a publisher sends raw request bytes on a base topic, and each subscriber
//! that handles it answers with a response envelope addressed to the caller.
//! Requests are correlated by the CID of their raw bytes.
//!
//! The module is transport-agnostic: it prepares publishes and correlates
//! deliveries; `kms::pubsub_transport` carries them over the node's topics.
//!
//! The wire-format constants:
//!
//! - Request ID: `CIDv1(raw, sha256(request_bytes))` — see [`id::derive_request_id`].
//! - Response envelope: `{ID: string, From: bytes, Data: bytes, Err: string}`
//!   encoded as dag-cbor with a definite-length map — see
//!   [`envelope::InternalResponse`].

pub mod correlator;
pub mod envelope;
pub mod id;

pub use correlator::{
    Correlator, PreparedPublish, PublishOptions, PubsubResponse, DEFAULT_MULTI_RESPONSE_BUFFER,
};
pub use envelope::InternalResponse;
pub use id::derive_request_id;

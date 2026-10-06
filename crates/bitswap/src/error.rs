//! Message-level errors.

use thiserror::Error;

/// Errors from building, encoding or decoding bitswap messages.
#[derive(Debug, Error)]
pub enum Error {
    /// Socket read failure.
    #[error("Error while reading from socket: {0}")]
    Read(#[from] std::io::Error),
    /// Protobuf decode failure.
    #[error("Error while decoding bitswap message: {0}")]
    Protobuf(#[from] prost::DecodeError),
    /// Invalid cid.
    #[error("Error while parsing cid: {0}")]
    Cid(#[from] cid::Error),
    /// Invalid multihash.
    #[error("Error while parsing multihash: {0}")]
    Multihash(#[from] multihash::Error),
    /// Multihash code with no known hasher.
    #[error("Unsupported multihash code {0}")]
    UnsupportedMultihashCode(u64),
    /// Unknown block presence enum value.
    #[error("Invalid block presence type {0}")]
    InvalidBlockPresenceType(i32),
    /// Unknown want type enum value.
    #[error("Invalid want type {0}")]
    InvalidWantType(i32),
}

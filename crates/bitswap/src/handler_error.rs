//! Errors raised while negotiating and driving a bitswap substream.

use libp2p::core::upgrade::NegotiationError;
use thiserror::Error;

use crate::error::Error;

/// Errors from a bitswap substream.
#[derive(Debug, Error)]
pub enum BitswapHandlerError {
    /// The message exceeds the maximum transmission size.
    #[error("max transmission size")]
    MaxTransmissionSize,
    /// Protocol negotiation timeout.
    #[error("negotiation timeout")]
    NegotiationTimeout,
    /// Protocol negotiation failed.
    #[error("negotatiation protocol error {0}")]
    NegotiationProtocolError(#[from] NegotiationError),
    /// IO error.
    #[error("io {0}")]
    Io(#[from] std::io::Error),
    /// Message error.
    #[error("bitswap {0}")]
    Bitswap(#[from] Error),
}

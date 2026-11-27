//! P2P endpoint handlers.
//!
//! These handlers provide HTTP access to P2P networking functionality:
//! - Node info (peer ID, addresses)
//! - Peer management (list, connect)
//! - Replicator management (list, add, remove)
//! - P2P collection management (list, add, remove)
//! - P2P document replication (list, add, remove, sync)
//!
//! All endpoints enforce NAC permissions when NAC is enabled.

use crate::error::{http_error_from_backend_message, peer_refused, HttpError};
use crate::router::{peer, P2PError};

mod collections;
mod documents;
mod manage;
mod peers;
mod replicators;

fn map_p2p_bad_request(error: P2PError) -> HttpError {
    match error {
        P2PError::InvalidInput(message) => http_error_from_backend_message(message),
        P2PError::NotFound(message) => HttpError::NotFound(message),
        P2PError::Unauthorized(message) => HttpError::Unauthorized(message),
        P2PError::Unsupported(message) => HttpError::NotImplemented(message),
        P2PError::Transport(message) => http_error_from_backend_message(message),
        P2PError::Internal(message) => HttpError::Internal(message),
    }
}

fn map_p2p_internal(error: P2PError) -> HttpError {
    match error {
        P2PError::InvalidInput(message) => HttpError::BadRequest(message),
        P2PError::NotFound(message) => HttpError::NotFound(message),
        P2PError::Unauthorized(message) => HttpError::Unauthorized(message),
        P2PError::Unsupported(message) => HttpError::NotImplemented(message),
        P2PError::Transport(message) => HttpError::Internal(message),
        P2PError::Internal(message) => HttpError::Internal(message),
    }
}

/// A refused sync: an adapter refusal, or a refusal before the adapter, by
/// the status its message names.
fn sync_refused(error: peer::Error) -> HttpError {
    match error {
        peer::Error::Refused(error) => map_p2p_internal(error),
        error => peer_refused(error),
    }
}

pub use collections::{
    add_collections, list_collections, remove_collections, sync_branchable, sync_versions,
};
pub use documents::{add_documents, list_documents, remove_documents, sync_documents};
pub use manage::{manage, manage_query};
pub use peers::{
    active_peers, connect, disconnect, get_info, get_shareable_address, peer_details, sync_status,
    P2pInfoResponse, ShareableAddressResponse,
};
pub use replicators::{
    add_replicator, list_replicators, remove_replicator, ReplicatorDeleteRequest, ReplicatorRequest,
};

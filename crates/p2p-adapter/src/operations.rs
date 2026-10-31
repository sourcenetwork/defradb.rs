//! The P2P operation surface every edge (HTTP, CLI, FFI, WASM, the embedded
//! node) calls: `P2POperations`, its inputs and errors, and the management
//! channel's requester.

use thiserror::Error;

/// The `bind_request` an operations trait provides: the handle that serves
/// one request (see `crate::request_scope`).
macro_rules! bind_request {
    ($operations:ident) => {
        /// This handle bound to the request `context` serves, for an
        /// implementation that reads it (the identity node access control
        /// checks, the creator of what it writes); `None` serves every request
        /// as it is.
        fn bind_request(
            &self,
            _context: &defra_core::RequestContext,
        ) -> Option<std::sync::Arc<dyn $operations>> {
            None
        }
    };
}

pub type ReplicationFilters = std::collections::BTreeMap<String, ReplicationFilter>;

/// HTTP wire shape for a per-collection replication filter.
///
/// Supports two forms:
/// - Legacy scalar equality: `{"Field": "f", "Value": <scalar>}`
/// - Rich predicate (any DefraDB query-filter conditions object):
///   `{"Conditions": {"f": {"_in": [...]}}}` or `{"predicate": {"f": {"_in": [...]}}}`
///
/// When `conditions` is present it takes precedence over `field`/`value`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReplicationFilter {
    #[serde(rename = "Field", default, skip_serializing_if = "String::is_empty")]
    pub field: String,
    #[serde(
        rename = "Value",
        default,
        skip_serializing_if = "serde_json::Value::is_null"
    )]
    pub value: serde_json::Value,
    /// Full query-filter conditions object for rich predicates (IN, composite, etc.).
    /// When present, `field`/`value` are ignored during conversion.
    #[serde(
        rename = "Conditions",
        alias = "predicate",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub conditions: Option<serde_json::Map<String, serde_json::Value>>,
}

impl Eq for ReplicationFilter {}

impl ReplicationFilter {
    /// Construct a simple scalar equality filter.
    pub fn eq(field: impl Into<String>, value: serde_json::Value) -> Self {
        Self {
            field: field.into(),
            value,
            conditions: None,
        }
    }

    /// Construct a rich predicate filter from a conditions map.
    pub fn predicate(conditions: serde_json::Map<String, serde_json::Value>) -> Self {
        Self {
            field: String::new(),
            value: serde_json::Value::Null,
            conditions: Some(conditions),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExplicitReplayCapabilityInput {
    #[serde(rename = "CollectionID")]
    pub collection_id: String,
    #[serde(rename = "Capability")]
    pub capability: String,
}

/// P2P error categories every edge maps to its own status or code.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum P2PError {
    #[error("invalid request: {0}")]
    InvalidInput(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("unsupported operation: {0}")]
    Unsupported(String),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type P2PResult<T> = Result<T, P2PError>;

impl From<String> for P2PError {
    fn from(message: String) -> Self {
        Self::Internal(message)
    }
}

impl From<&str> for P2PError {
    fn from(message: &str) -> Self {
        Self::Internal(message.to_string())
    }
}

impl P2PError {
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported(message.into())
    }
}

/// Raw transport-specific peer identifier.
///
/// This is intentionally distinct from the address strings returned by
/// [`P2POperations::connected_peers`]. Concrete adapters perform their own
/// transport-specific parse and canonicalization before issuing a challenge.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct TransportPeerId(String);

impl TransportPeerId {
    /// Constructs a canonical raw transport peer identifier.
    ///
    /// Empty values, surrounding whitespace, and address-like values containing
    /// `/` are rejected as [`P2PError::InvalidInput`]. Concrete transport
    /// adapters perform their stricter transport-specific parse afterward.
    pub fn new(value: impl Into<String>) -> P2PResult<Self> {
        let value = value.into();
        if value.is_empty() || value.trim() != value || value.contains('/') {
            return Err(P2PError::InvalidInput(
                "transport peer ID must be a non-empty canonical raw ID".to_string(),
            ));
        }
        Ok(Self(value))
    }

    /// Returns the validated raw transport peer identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for TransportPeerId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for TransportPeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Trait for P2P operations that can be accessed via HTTP.
///
/// Abstracts P2P host functionality to decouple HTTP handlers from the
/// actual P2P implementation, enabling both dependency injection and testing.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait P2POperations: defra_core::thread_bounds::MaybeSendSync {
    bind_request!(P2POperations);

    /// Get the local peer ID.
    async fn local_peer_id(&self) -> P2PResult<String>;

    /// Get listening addresses.
    async fn listen_addresses(&self) -> P2PResult<Vec<String>>;

    /// Get the single best address to share with another node, if available.
    ///
    /// This is a stronger contract than `listen_addresses()`: callers should not
    /// need to guess which returned address is meant for remote sharing.
    ///
    /// Transports that do not have a clear shareable-address concept may return
    /// `Ok(None)`.
    async fn shareable_address(&self) -> P2PResult<Option<String>> {
        Ok(None)
    }

    /// Get connected peers.
    async fn connected_peers(&self) -> P2PResult<Vec<String>>;

    /// Resolve the Defra DID authenticated by a connected transport peer.
    ///
    /// Implementations must actively challenge the peer and verify the
    /// returned identity token against the local transport identity. A cached
    /// result is acceptable only while its verified token remains fresh and
    /// the connection remains live. `Ok(None)` means the peer explicitly has
    /// no configured Defra identity; malformed tokens, wrong audiences,
    /// protocol failures, and remote internal failures are errors. The input
    /// is a canonical raw transport peer ID, never an address returned by
    /// `connected_peers`. Requires `P2pPeerActive`; transports without an
    /// authenticated identity exchange fail closed.
    async fn resolve_peer_identity(
        &self,
        _peer_id: &TransportPeerId,
    ) -> P2PResult<Option<identity::Did>> {
        Err(P2PError::unsupported(
            "authenticated peer identity resolution is unavailable",
        ))
    }

    /// Live snapshot of sync resource state (push backlog occupancy,
    /// per-peer backlog, pending DAGs, overload counters) for diagnostics
    /// and downstream conformance (#1099). The JSON shape is owned by
    /// `p2p::sync::SyncStatus`. Transports without a sync coordinator
    /// return `Null`.
    async fn sync_status(&self) -> P2PResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }

    /// Everything this node knows about one peer, for debugging: connection
    /// path and traffic, connect history, per-direction sync outcomes with
    /// the last error, push queue and durable retries, pending and
    /// quarantined fetches, and the peer's DID when connected. The JSON
    /// shape is owned by `p2p::sync::PeerDetails`; the summaries in
    /// `sync_status().peers` are the overview. The input is a canonical raw
    /// transport peer ID. Transports without a sync coordinator return
    /// `Null`.
    async fn peer_details(&self, _peer_id: &TransportPeerId) -> P2PResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }

    /// Connect to a peer at the given address.
    async fn connect_peer(&self, addr: &str) -> P2PResult<()>;

    /// Disconnect the live connection to the peer at the given address.
    async fn disconnect_peer(&self, addr: &str) -> P2PResult<()>;

    /// Authorize a peer to open an inbound connection to this node while it
    /// is running, without a restart.
    ///
    /// Widens who may connect in: it does not itself dial, connect to, or
    /// disconnect from the peer, and it is a no-op when the transport already
    /// accepts every inbound peer. The input is a canonical raw transport
    /// peer ID, never an address returned by `connected_peers`. Transports
    /// without an inbound allowlist concept return an unsupported error.
    async fn allow_peer(&self, _peer_id: &TransportPeerId) -> P2PResult<()> {
        Err(P2PError::unsupported(
            "inbound peer allowlisting is unavailable",
        ))
    }

    /// Bar a peer from this node in both directions, while it is running and
    /// without a restart.
    ///
    /// Symmetric with [`Self::allow_peer`] in intent but not in reach, and
    /// deliberately stronger. Narrowing who may connect IN is not enough to
    /// count as a revocation: a node dials peers on its own initiative, so an
    /// implementation must also refuse its OWN outbound connections to the
    /// peer, or its reconnect logic undoes the revocation on the next sweep.
    /// It must additionally close every connection the peer currently holds,
    /// and it must record the bar BEFORE closing anything, so a reconnect
    /// racing the call cannot be re-admitted in between.
    ///
    /// It does not guarantee an in-flight request already being served over
    /// such a connection is aborted mid-exchange.
    ///
    /// Revocation is expected to be durable rather than cosmetic: an
    /// implementation should also drop state that would make this node reach
    /// for the peer again on a timer, such as a replicator registration.
    /// That makes the call destructive, and NOT the inverse of `allow_peer`:
    /// re-admitting a peer restores its ability to connect, not whatever was
    /// deregistered.
    ///
    /// The input is a canonical raw transport peer ID, never an address
    /// returned by `connected_peers`. Transports without an inbound
    /// allowlist concept return an unsupported error.
    async fn deny_peer(&self, _peer_id: &TransportPeerId) -> P2PResult<()> {
        Err(P2PError::unsupported(
            "inbound peer allowlisting is unavailable",
        ))
    }

    /// Notify the transport that local network conditions may have changed.
    ///
    /// Some transports, such as iroh, use this to refresh relay/direct
    /// connectivity after interface changes. Transports that do not require
    /// explicit handling may treat this as a no-op.
    async fn notify_network_change(&self) -> P2PResult<()> {
        Ok(())
    }

    /// Get all replicators.
    async fn get_replicators(&self) -> P2PResult<Vec<ReplicatorInfo>>;

    /// Add a replicator for collections.
    async fn add_replicator(
        &self,
        collections: Vec<String>,
        addr: Option<&str>,
        filters: ReplicationFilters,
        explicit_replay_capabilities: Vec<ExplicitReplayCapabilityInput>,
        expected_authorizer_did: Option<&str>,
    ) -> P2PResult<()>;

    /// Remove a replicator for collections.
    async fn remove_replicator(
        &self,
        collections: Vec<String>,
        addr: Option<&str>,
    ) -> P2PResult<()>;

    /// Get P2P collections.
    async fn get_collections(&self) -> P2PResult<Vec<String>>;

    /// Add collections to P2P.
    async fn add_collections(&self, collections: Vec<String>) -> P2PResult<()>;

    /// Remove collections from P2P.
    async fn remove_collections(&self, collections: Vec<String>) -> P2PResult<()>;

    /// Get P2P documents (for document-level replication).
    async fn get_documents(&self) -> P2PResult<Vec<P2pDocumentInfo>>;

    /// Add documents to P2P replication.
    async fn add_documents(&self, docs: Vec<P2pDocumentRequest>) -> P2PResult<()>;

    /// Remove documents from P2P replication.
    async fn remove_documents(&self, docs: Vec<P2pDocumentRequest>) -> P2PResult<()>;

    /// Sync specific documents from connected peers.
    ///
    /// `timeout` is the caller's deadline for the whole operation. `None` means
    /// `DEFAULT_DOC_SYNC_TIMEOUT`, matching the 5s Go falls back to when its
    /// inherited context carries no deadline.
    async fn sync_documents(
        &self,
        collection_name: &str,
        doc_ids: Vec<String>,
        timeout: Option<std::time::Duration>,
    ) -> P2PResult<()>;

    /// For each of `doc_ids`, whether replicator `peer_id` acknowledged the
    /// heads this node currently holds for it, with no retry outstanding. A
    /// newer local write reads `false` until the peer acknowledges again.
    async fn replication_acknowledged(
        &self,
        _peer_id: &str,
        _collection_name: &str,
        _doc_ids: &[String],
    ) -> P2PResult<Vec<bool>> {
        Err(P2PError::unsupported(
            "replication acknowledgement is unavailable",
        ))
    }

    /// Whether each selected document's current composite heads were acknowledged.
    /// Unlike collection replication, a selective push never acknowledges the
    /// other documents or heads of a branchable collection.
    async fn document_heads_acknowledged(
        &self,
        _peer_id: &str,
        _collection_name: &str,
        _doc_ids: &[String],
    ) -> P2PResult<Vec<bool>> {
        Err(P2PError::unsupported(
            "document head acknowledgement is unavailable",
        ))
    }

    /// Sync a branchable collection from connected peers.
    async fn sync_branchable_collection(&self, collection_id: &str) -> P2PResult<()>;

    /// Sync collection versions (schema definitions) from connected peers via Bitswap.
    async fn sync_collection_versions(&self, version_ids: Vec<String>) -> P2PResult<()>;

    /// Send an explicit document set to one peer, without a subscription.
    ///
    /// `Ok` means the peer acknowledged every listed document's current heads
    /// in this call; each call sends them all again, whatever an earlier call
    /// achieved. No retry marker is registered and no replicator's filter or
    /// retry schedule applies or changes, so an `Err` leaves nothing behind to
    /// resume: the caller owns calling again. The node owns the send pacing,
    /// slot counts and per-send deadline. The first send deadline, lost
    /// connection or receiver pause hint stops the call early; dropping the
    /// future stops it too, without waiting for the sends already on the wire,
    /// which end on their own deadline. Default implementations report the
    /// operation as unsupported.
    async fn push_documents_to_peer(
        &self,
        _peer_id: &str,
        _docs: Vec<P2pDocumentRequest>,
    ) -> P2PResult<()> {
        Err(P2PError::unsupported(
            "push_documents_to_peer is not implemented",
        ))
    }
}

/// Replicator information for HTTP responses.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReplicatorInfo {
    pub id: Option<String>,
    pub collections: Vec<String>,
    pub address: Option<String>,
    pub status: Option<u8>,
    pub last_status_change: Option<String>,
    pub filters: ReplicationFilters,
}

/// P2P document information for HTTP responses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct P2pDocumentInfo {
    /// Collection name the document belongs to.
    #[serde(rename = "Collection")]
    pub collection: String,
    /// Document ID.
    #[serde(rename = "DocID")]
    pub doc_id: String,
}

/// Request to add/remove P2P documents (Go-compatible format).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct P2pDocumentRequest {
    /// Collection name the document belongs to.
    #[serde(rename = "Collection")]
    pub collection: String,
    /// Document ID.
    #[serde(rename = "DocID")]
    pub doc_id: String,
}

/// Request body for document sync (Go-compatible).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncDocumentsRequest {
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    #[serde(rename = "docIDs")]
    pub doc_ids: Vec<String>,
    /// Go duration string (`"5s"`, `"1m30s"`). Go's client sends this whenever
    /// the caller's context carries a deadline.
    #[serde(rename = "timeout", default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

/// Request body for branchable collection sync (Go-compatible).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncBranchableRequest {
    #[serde(rename = "collectionID")]
    pub collection_id: String,
}

/// Request body for collection version sync (Go-compatible).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncVersionsRequest {
    #[serde(rename = "versionIDs")]
    pub version_ids: Vec<String>,
}

/// A management operation to relay to a remote P2P peer (http-native mirror of
/// the p2p manage ops; http does not depend on p2p).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "Kind")]
pub enum RemoteManageOp {
    ReplicatorAdd {
        addresses: Vec<String>,
        collection_ids: Vec<String>,
        #[serde(default)]
        filters: ReplicationFilters,
    },
    ReplicatorDelete {
        addresses: Vec<String>,
        collection_ids: Vec<String>,
    },
    CollectionAdd {
        collection_ids: Vec<String>,
    },
    CollectionRemove {
        collection_ids: Vec<String>,
    },
    DocumentAdd {
        docs: Vec<RemoteManageDocRef>,
    },
    DocumentRemove {
        docs: Vec<RemoteManageDocRef>,
    },
    PeerConnect {
        address: String,
    },
    PeerDisconnect {
        address: String,
    },
}

/// A document reference for [`RemoteManageOp`] document operations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoteManageDocRef {
    pub collection: String,
    pub doc_id: String,
}

/// A read-only management operation to relay to a remote P2P peer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "Kind")]
pub enum RemoteManageQueryOp {
    ReplicatorList,
    CollectionList,
    DocumentList,
}

/// Typed result of a [`RemoteManageQueryOp`].
///
/// Serialize-only because it embeds [`ReplicatorInfo`] (a response type), and is
/// only ever produced by the requester and rendered into an HTTP response.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "Kind")]
pub enum RemoteManageQueryResult {
    Replicators { replicators: Vec<ReplicatorInfo> },
    Strings { values: Vec<String> },
    Documents { documents: Vec<RemoteManageDocRef> },
}

/// Wire sentinel for a remote NAC denial on the management channel.
///
/// This is the contract between the manage serve side (which produces it when
/// authorization fails), the [`ManageRequester`] implementation (which
/// normalizes denials to it), and the HTTP handler (which maps it to 403). It is
/// the single source of truth for the `"unauthorized"` string.
pub const MANAGE_UNAUTHORIZED: &str = "unauthorized";

/// Relays management requests to P2P-only peers on behalf of an HTTP caller.
///
/// In the management deployment model, the target node (B) exposes only its P2P
/// port. To manage B, a caller hits this node's (A's) HTTP API; this node then
/// sends a signed P2P management request to B, relaying the caller-minted actor
/// token (a JWT with `aud` = B's peer-id).
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait ManageRequester: defra_core::thread_bounds::MaybeSendSync {
    /// Relay a mutating management request to the peer at `target_addr`.
    ///
    /// `target_addr` is the peer's shareable address (this node dials it).
    /// `auth_token` is the caller-minted JWT (`aud` = the target peer-id).
    /// Returns `Ok(())` on success, or an error string (`"unauthorized"` when
    /// the remote NAC denies the operation).
    async fn manage(
        &self,
        target_addr: &str,
        auth_token: Vec<u8>,
        op: RemoteManageOp,
    ) -> Result<(), String>;

    /// Relay a read-only management query to the peer at `target_addr`.
    async fn manage_query(
        &self,
        target_addr: &str,
        auth_token: Vec<u8>,
        op: RemoteManageQueryOp,
    ) -> Result<RemoteManageQueryResult, String>;
}

#[cfg(test)]
mod transport_peer_id_tests {
    use super::TransportPeerId;

    #[test]
    fn transport_peer_id_rejects_addresses_whitespace_and_empty_values() {
        assert!(TransportPeerId::new("").is_err());
        assert!(TransportPeerId::new(" peer-id").is_err());
        assert!(TransportPeerId::new("peer-id ").is_err());
        assert!(TransportPeerId::new("/ip4/127.0.0.1/p2p/peer-id").is_err());
        assert_eq!(TransportPeerId::new("peer-id").unwrap().as_str(), "peer-id");
        assert!(serde_json::from_str::<TransportPeerId>(r#""""#).is_err());
        assert!(serde_json::from_str::<TransportPeerId>(r#"" peer-id""#).is_err());
        assert!(
            serde_json::from_str::<TransportPeerId>(r#""/ip4/127.0.0.1/p2p/peer-id""#).is_err()
        );
    }
}

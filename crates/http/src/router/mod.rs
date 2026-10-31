//! Router configuration and route definitions.

#[cfg(feature = "server")]
mod routes;
#[cfg(feature = "server")]
mod state;
mod traits;

#[cfg_attr(not(feature = "server"), allow(unused_imports))]
pub(crate) use defra_p2p_adapter::{
    peer, ExplicitReplayCapabilityInput, ManageRequester, P2PError, P2POperations, RemoteManageOp,
    RemoteManageQueryOp, RemoteManageQueryResult, ReplicationFilters, SyncBranchableRequest,
    SyncDocumentsRequest, SyncVersionsRequest, TransportPeerId, MANAGE_UNAUTHORIZED,
};
#[cfg(any(test, feature = "test-utils"))]
pub(crate) use defra_p2p_adapter::{
    P2PResult, P2pDocumentInfo, P2pDocumentRequest, ReplicatorInfo,
};
#[cfg(feature = "server")]
pub use routes::{create_router, create_router_with_rest, create_router_with_state};
#[cfg(feature = "server")]
pub(crate) use routes::{create_router_with_state_and_body_limits, BodyLimits};
#[cfg(feature = "server")]
pub use state::{AppState, AppStateBuilder};
pub use traits::{
    BackupOperations, BlockOperations, CollectionManagementOperations, CollectionOperations,
    DocumentAcpOperations, DumpOperations, EncryptedIndexInfo, EncryptedIndexOperations,
    ImportResult, IndexFieldInfo, IndexInfo, IndexOperations, NacStatus, NacStatusInfo,
    NodeAcpOperations, NodePermission, ViewOperations,
};

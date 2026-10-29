//! Router configuration and route definitions.

pub mod peer;
#[cfg(feature = "server")]
mod routes;
#[cfg(feature = "server")]
mod state;
mod traits;

#[cfg(feature = "server")]
pub use routes::{create_router, create_router_with_rest, create_router_with_state};
#[cfg(feature = "server")]
pub(crate) use routes::{create_router_with_state_and_body_limits, BodyLimits};
#[cfg(feature = "server")]
pub use state::{AppState, AppStateBuilder};
pub use traits::{
    AcpLightClientStatus, AcpOperations, BackupOperations, BlockOperations,
    CollectionManagementOperations, CollectionOperations, DocumentAcpOperations, DumpOperations,
    EncryptedIndexInfo, EncryptedIndexOperations, ExplicitReplayCapabilityInput, ImportResult,
    IndexFieldInfo, IndexInfo, IndexOperations, ManageRequester, NacStatus, NacStatusInfo,
    NodeAcpOperations, NodePermission, P2PError, P2POperations, P2PResult, P2pDocumentInfo,
    P2pDocumentRequest, PolicyInfo, RemoteManageDocRef, RemoteManageOp, RemoteManageQueryOp,
    RemoteManageQueryResult, ReplicationFilter, ReplicationFilters, ReplicatorInfo,
    SyncBranchableRequest, SyncDocumentsRequest, SyncVersionsRequest, TransportPeerId,
    ViewOperations, MANAGE_UNAUTHORIZED,
};

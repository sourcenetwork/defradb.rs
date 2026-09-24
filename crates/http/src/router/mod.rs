//! Router configuration and route definitions.

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
    CollectionManagementOperations, CollectionVersionOperations, DocumentAcpOperations,
    DumpOperations, EncryptedIndexInfo, EncryptedIndexOperations, ExplicitReplayCapabilityInput,
    ImportResult, IndexFieldInfo, IndexInfo, IndexOperations, LensOperations, ManageRequester,
    NacStatus, NacStatusInfo, NodeAcpOperations, NodePermission, P2PError, P2POperations,
    P2PResult, P2pDocumentInfo, P2pDocumentRequest, PolicyInfo, RemoteManageDocRef, RemoteManageOp,
    RemoteManageQueryOp, RemoteManageQueryResult, ReplicationFilter, ReplicationFilters,
    ReplicatorInfo, SchemaOperations, SyncBranchableRequest, SyncDocumentsRequest,
    SyncVersionsRequest, TransactionOperations, TransportPeerId, ViewOperations,
    MANAGE_UNAUTHORIZED,
};

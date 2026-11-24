//! The routes the server serves, one row per method.
//!
//! Kept out of `route_permissions.rs` so the table stays one line per route:
//! formatting it as a function call explodes each row into six lines, and a
//! reviewer needs to be able to scan it for a missing route. `cargo fmt` skips
//! the table and formats everything else.

use axum::http::Method;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde_json::{Map, Value};

use crate::handlers::acp::{
    AddPolicyResponse, DocDecisionRequest, DocDecisionResponse, DocRelationshipDeleteResponse,
    DocRelationshipRequest, DocRelationshipResponse, OwnDocumentsRequest, OwnDocumentsResponse,
    PolicyInfo,
};
use crate::handlers::backup::ExportRequest;
use crate::handlers::batch::{BatchSignResponse, BatchVerifyRequest};
use crate::handlers::block::SignedBlockResponse;
use crate::handlers::ccip::CcipResponse;
use crate::handlers::collections::PatchCollectionRequest;
use crate::handlers::collections::{CollectionDocIdsResponse, PurgeDocumentsResponse};
use crate::handlers::graphql::TransactionalQueryRequest;
use crate::handlers::index::{GoCreateIndexRequest, GoIndexDescription, GoListIndexesResult};
use crate::handlers::lens::{AddLensRequest, SetMigrationResponse};
use crate::handlers::nac::{
    AddActorRelationshipResult, DeleteActorRelationshipResult, EnableNacRequest,
    GoNacRelationshipRequest, NacStatusResult,
};
use crate::handlers::p2p::ReplicatorDeleteRequest;
use crate::handlers::p2p::{ManageQueryRequestBody, ManageRequestBody, ShareableAddressResponse};
use crate::handlers::utility::NodeIdentityResponse;
use crate::handlers::views::ViewNamesRequest;
use crate::route_permissions::RoutePermission;
use crate::router::NodePermission;
use crate::wire::{
    ActionExecution, DocumentRelationship, DocumentsResult, LightClientStatus,
    PermissionProvenance, SourcePermission, VersionInfo,
};
use schema::CollectionVersion;

use RoutePermission::{Dynamic, Exempt, IdentityOnly};

/// One route the server serves, and what the auth middleware needs to know
/// about it.
///
/// No `PartialEq`: two bodies are `fn` pointers, whose addresses are not
/// guaranteed unique, so comparing whole rows would not mean what it reads as.
#[derive(Debug, Clone)]
pub struct Route {
    /// The Axum `MatchedPath` template, in its `/api/v0` form. Mounts are
    /// folded onto that form by `route_permissions::normalize_api_version`
    /// before lookup.
    pub path: &'static str,
    /// The method the handler is registered for, or `None` for the one route
    /// registered with `any()` (the GraphQL websocket), which takes all.
    pub method: Option<Method>,
    /// What the edge enforces before the handler runs.
    pub permission: RoutePermission,
    /// Whether the route reads its collection before it runs, as Go's
    /// `CollectionMiddleware` does for the collection handler's routes
    /// (`http/handler.go`): such a route checks `get-collection` before its
    /// own permission.
    pub collection_first: bool,
    /// The JSON body the route accepts, as a schema rendered with the
    /// document's generator, or `None` for a route that takes no body.
    pub request: Option<BodySchema>,
    /// Whether the handler refuses a request with no body. `views/gc` reads an
    /// absent body as "every view", so its document says `required: false`.
    pub request_required: bool,
    /// The JSON body the route returns, the same way.
    pub response: Option<BodySchema>,
}

/// A body's schema, rendered with the document's [`SchemaGenerator`] so the
/// definitions it reaches land in one `components.schemas`.
pub type BodySchema = fn(&mut SchemaGenerator) -> Schema;

/// Renders `T`'s schema. Used as [`BodySchema`]: a row names the type, and the
/// schema is built when a client asks for the document.
pub fn body<T>(generator: &mut SchemaGenerator) -> Schema
where
    T: JsonSchema,
{
    generator.root_schema_for::<T>()
}

impl Route {
    /// Whether this row is the lookup for `method` at its path.
    pub(crate) fn matches(&self, method: &Method) -> bool {
        self.method.as_ref().is_none_or(|own| own == method)
    }
}

// The method a row is registered for. [`ANY`] is the one route registered with
// `any()`, which takes every method. See `Route::method`.
const GET: Option<Method> = Some(Method::GET);
const POST: Option<Method> = Some(Method::POST);
const PATCH: Option<Method> = Some(Method::PATCH);
const DELETE: Option<Method> = Some(Method::DELETE);
const ANY: Option<Method> = None;

/// Shorthand for the permission of most routes.
const fn required(permission: NodePermission) -> RoutePermission {
    RoutePermission::Required(permission)
}

/// One row of [`ROUTES`]. `method` names the registered method, `permission`
/// what the edge enforces, and `collection_first` whether `get-collection` is
/// checked first.
const fn row(
    path: &'static str,
    method: Option<Method>,
    permission: RoutePermission,
    collection_first: bool,
) -> Route {
    Route {
        path,
        method,
        permission,
        collection_first,
        request: None,
        request_required: true,
        response: None,
    }
}

/// A row whose request and response bodies are typed.
const fn row_body(
    path: &'static str,
    method: Option<Method>,
    permission: RoutePermission,
    collection_first: bool,
    request: Option<BodySchema>,
    response: Option<BodySchema>,
) -> Route {
    Route {
        path,
        method,
        permission,
        collection_first,
        request,
        request_required: true,
        response,
    }
}

/// A row whose request body is optional: the handler runs with none.
const fn row_optional_body(
    path: &'static str,
    method: Option<Method>,
    permission: RoutePermission,
    collection_first: bool,
    request: BodySchema,
) -> Route {
    Route {
        path,
        method,
        permission,
        collection_first,
        request: Some(request),
        request_required: false,
        response: None,
    }
}

/// Every route the server serves, one row per method, `api_routes` in
/// `router/routes.rs` order.
///
/// The server mounts `api_routes` at every prefix in
/// [`crate::go_paths::API_PREFIXES`]; the rows carry the `/api/v0` form and
/// `route_permissions::normalize_api_version` folds the other mounts onto it.
#[rustfmt::skip]
pub const ROUTES: &[Route] = &[
    // Exempt routes (no auth needed)
    row("/health-check", GET, Exempt, false),
    row("/openapi.json", GET, Exempt, false),
    row("/openapi.rs.json", GET, Exempt, false),
    row_body("/api/v0/version", GET, Exempt, false, None, Some(body::<VersionInfo>)),
    row("/api/v0/graphql/ws", ANY, Exempt, false),
    row_body("/api/v0/batch/verify", POST, Exempt, false, Some(body::<BatchVerifyRequest>), None),
    // GraphQL
    row("/api/v0/graphql/client-sign", GET, required(NodePermission::DocumentUpdate), false),
    // GET is always a read operation; POST determines permission by parsing
    // the query in the handler.
    row("/api/v0/graphql", GET, required(NodePermission::DocumentRead), false),
    row_body("/api/v0/graphql", POST, Dynamic, false, Some(body::<TransactionalQueryRequest>), None),
    row_body("/api/v0/ccip", POST, Dynamic, false, None, Some(body::<CcipResponse>)),
    row("/api/v0/ccip/{sender}/{data}", GET, Dynamic, false),
    row("/api/v0/events", GET, Dynamic, false),
    row_body("/api/v0/actions", GET, required(NodePermission::ActionList), false, None, Some(body::<Vec<ActionExecution>>)),
    row("/api/v0/schema", GET, required(NodePermission::CollectionGet), false),
    // Transactions (permissions enforced per-operation within tx)
    row("/api/v0/tx", POST, IdentityOnly, false),
    row("/api/v0/tx/{id}", POST, IdentityOnly, false),
    row("/api/v0/tx/{id}", DELETE, IdentityOnly, false),
    // Collections
    row_body("/api/v0/collections", GET, required(NodePermission::CollectionGet), false, None, Some(body::<Vec<String>>)),
    row_body("/api/v0/collections", POST, required(NodePermission::CollectionPatch), false, None, Some(body::<Vec<CollectionVersion>>)),
    row_body("/api/v0/collections", PATCH, required(NodePermission::CollectionPatch), false, Some(body::<PatchCollectionRequest>), None),
    // Dropping collections by name. This fell to the read default while
    // `delete_collections_by_names` enforced `CollectionPatch` itself, so the
    // outer gate was weaker than the handler.
    row("/api/v0/collections", DELETE, required(NodePermission::CollectionPatch), false),
    row("/api/v0/collections/default", POST, required(NodePermission::CollectionPatch), false),
    row_body("/api/v0/collections/versions", DELETE, required(NodePermission::CollectionPatch), false, Some(body::<Vec<String>>), Some(body::<serde_json::Value>)),
    row_body("/api/v0/collections/migrations", POST, required(NodePermission::MigrationSet), false, None, Some(body::<SetMigrationResponse>)),
    row("/api/v0/collections/indexes", GET, required(NodePermission::IndexList), false),
    row_body("/api/v0/collections/{name}", GET, required(NodePermission::CollectionGet), false, None, Some(body::<CollectionDocIdsResponse>)),
    // POST and PATCH here are Go's filtered document operations, not
    // collection ones, so they are document permissions. Dropping a collection
    // is `DELETE /collections?name=...`, which keeps `CollectionPatch`.
    row_body("/api/v0/collections/{name}", POST, required(NodePermission::DocumentUpdate), true, None, Some(body::<Vec<String>>)),
    row_body(
        "/api/v0/collections/{name}",
        PATCH,
        required(NodePermission::DocumentUpdate),
        true,
        None,
        Some(body::<DocumentsResult>),
    ),
    row_body(
        "/api/v0/collections/{name}",
        DELETE,
        required(NodePermission::DocumentDelete),
        true,
        None,
        Some(body::<DocumentsResult>),
    ),
    row_body("/api/v0/collections/{name}/describe", GET, required(NodePermission::CollectionGet), false, None, Some(body::<serde_json::Value>)),
    row_body("/api/v0/collections/{name}/exists", GET, required(NodePermission::CollectionGet), false, None, Some(body::<serde_json::Value>)),
    row_body("/api/v0/collections/{name}/truncate", DELETE, required(NodePermission::CollectionTruncate), true, None, Some(body::<()>)),
    // A purge is checked against the node permission of a truncate.
    row_body("/api/v0/collections/{name}/documents/purge", DELETE, required(NodePermission::CollectionTruncate), false, None, Some(body::<PurgeDocumentsResponse>)),
    row_body("/api/v0/collections/{name}/document/{docID}", GET, required(NodePermission::DocumentRead), true, None, Some(body::<Value>)),
    row("/api/v0/collections/{name}/document/{docID}", PATCH, required(NodePermission::DocumentUpdate), true),
    row("/api/v0/collections/{name}/document/{docID}", DELETE, required(NodePermission::DocumentDelete), true),
    row_body("/api/v0/collections/{name}/indexes", GET, required(NodePermission::IndexList), true, None, Some(body::<Vec<GoListIndexesResult>>)),
    row_body("/api/v0/collections/{name}/indexes", POST, required(NodePermission::IndexCreate), true, Some(body::<GoCreateIndexRequest>), Some(body::<GoIndexDescription>)),
    row("/api/v0/collections/{name}/indexes/{index}", DELETE, required(NodePermission::IndexDelete), true),
    // The edge checks `get-collection` first and leaves the encrypted index
    // permission to the operation, so it comes after the collection on every
    // server, as in Go.
    row("/api/v0/collections/{name}/encrypted-indexes", GET, IdentityOnly, true),
    row("/api/v0/collections/{name}/encrypted-indexes", POST, IdentityOnly, true),
    row("/api/v0/collections/{name}/encrypted-indexes/{field}", DELETE, IdentityOnly, true),
    // P2P
    row("/api/v0/p2p/info", GET, required(NodePermission::P2pPeerInfo), false),
    row_body("/api/v0/p2p/sync/status", GET, required(NodePermission::P2pPeerInfo), false, None, Some(body::<serde_json::Value>)),
    row_body("/api/v0/p2p/shareable-address", GET, required(NodePermission::P2pPeerInfo), false, None, Some(body::<ShareableAddressResponse>)),
    row_body("/api/v0/p2p/active-peers", GET, required(NodePermission::P2pPeerActive), false, None, Some(body::<Vec<String>>)),
    row_body("/api/v0/p2p/connect", POST, required(NodePermission::P2pPeerConnect), false, Some(body::<Vec<String>>), None),
    row_body("/api/v0/p2p/disconnect", POST, required(NodePermission::P2pPeerDisconnect), false, Some(body::<Vec<String>>), None),
    row_body("/api/v0/p2p/peers/{peer_id}", GET, required(NodePermission::P2pPeerInfo), false, None, Some(body::<serde_json::Value>)),
    row("/api/v0/p2p/replicators", GET, required(NodePermission::P2pReplicatorList), false),
    row_body("/api/v0/p2p/replicators", POST, required(NodePermission::P2pReplicatorAdd), false, None, Some(body::<()>)),
    row_body("/api/v0/p2p/replicators", DELETE, required(NodePermission::P2pReplicatorDelete), false, Some(body::<ReplicatorDeleteRequest>), None),
    row_body("/api/v0/p2p/collections", GET, required(NodePermission::P2pCollectionList), false, None, Some(body::<Vec<String>>)),
    row_body("/api/v0/p2p/collections", POST, required(NodePermission::P2pCollectionAdd), false, Some(body::<Vec<String>>), None),
    row_body("/api/v0/p2p/collections", DELETE, required(NodePermission::P2pCollectionDelete), false, Some(body::<Vec<String>>), None),
    row_body("/api/v0/p2p/collections/sync-branchable", POST, required(NodePermission::P2pSyncBranchableCollection), false, None, Some(body::<()>)),
    row_body("/api/v0/p2p/collections/sync-versions", POST, required(NodePermission::P2pSyncCollectionVersions), false, None, Some(body::<()>)),
    row_body("/api/v0/p2p/documents", GET, required(NodePermission::P2pDocumentList), false, None, Some(body::<Vec<String>>)),
    row_body("/api/v0/p2p/documents", POST, required(NodePermission::P2pDocumentAdd), false, Some(body::<Vec<String>>), None),
    row_body("/api/v0/p2p/documents", DELETE, required(NodePermission::P2pDocumentDelete), false, Some(body::<Vec<String>>), None),
    row_body("/api/v0/p2p/documents/sync", POST, required(NodePermission::P2pSyncDocuments), false, None, Some(body::<()>)),
    // Management relay: this node connects to and commands the target peer, so
    // it reuses the peer-connect permission to prevent an open relay.
    row_body("/api/v0/p2p/manage", POST, required(NodePermission::P2pPeerConnect), false, Some(body::<ManageRequestBody>), None),
    row_body("/api/v0/p2p/manage/query", POST, required(NodePermission::P2pPeerConnect), false, Some(body::<ManageQueryRequestBody>), Some(body::<crate::wire::RemoteManageQueryResult>)),
    // ACP (Document Access Control)
    row_body("/api/v0/acp/status", GET, required(NodePermission::DacStatus), false, None, Some(body::<LightClientStatus>)),
    row_body("/api/v0/acp/document/policy", POST, required(NodePermission::DacPolicyAdd), false, None, Some(body::<AddPolicyResponse>)),
    row_body("/api/v0/acp/policy", GET, required(NodePermission::DacStatus), false, None, Some(body::<Vec<PolicyInfo>>)),
    row_body("/api/v0/acp/policy/{id}", GET, required(NodePermission::DacStatus), false, None, Some(body::<PolicyInfo>)),
    row_body("/api/v0/acp/document/decide", POST, required(NodePermission::DacStatus), false, Some(body::<DocDecisionRequest>), Some(body::<DocDecisionResponse>)),
    // Reads the documents in the caller's transaction, as a query does.
    row_body("/api/v0/acp/document/is-owner", POST, required(NodePermission::DocumentRead), false, Some(body::<OwnDocumentsRequest>), Some(body::<OwnDocumentsResponse>)),
    row_body("/api/v0/acp/document/relationship", GET, Dynamic, false, None, Some(body::<bool>)),
    row_body("/api/v0/acp/document/relationship", POST, required(NodePermission::DacRelationAdd), false, Some(body::<DocRelationshipRequest>), Some(body::<DocRelationshipResponse>)),
    row_body("/api/v0/acp/document/relationship", DELETE, required(NodePermission::DacRelationDelete), false, Some(body::<DocRelationshipRequest>), Some(body::<DocRelationshipDeleteResponse>)),
    // Listing is gated by the document's owner or managers in the core.
    row_body(
        "/api/v0/acp/document/relationships",
        GET,
        Dynamic,
        false,
        None,
        Some(body::<Vec<DocumentRelationship>>),
    ),
    row_body("/api/v0/acp/document/relationships", POST, required(NodePermission::DacRelationAdd), false, Some(body::<Vec<DocRelationshipRequest>>), Some(body::<Vec<DocRelationshipResponse>>)),
    // ACP Node (NAC via Go-compatible /acp/node/* routes)
    row_body("/api/v0/acp/node/status", GET, required(NodePermission::NacStatus), false, None, Some(body::<NacStatusResult>)),
    // The subject itself, an admin or a `NacStatus` holder: decided in the core.
    row_body(
        "/api/v0/acp/node/explain",
        GET,
        Dynamic,
        false,
        None,
        Some(body::<Vec<PermissionProvenance>>),
    ),
    row_body("/api/v0/acp/node/enable", POST, Dynamic, false, Some(body::<EnableNacRequest>), None),
    row_body("/api/v0/acp/node/relationship", POST, required(NodePermission::NacRelationAdd), false, Some(body::<GoNacRelationshipRequest>), Some(body::<AddActorRelationshipResult>)),
    row_body("/api/v0/acp/node/relationship", DELETE, required(NodePermission::NacRelationDelete), false, Some(body::<GoNacRelationshipRequest>), Some(body::<DeleteActorRelationshipResult>)),
    row_body("/api/v0/acp/node/relationships", POST, required(NodePermission::NacRelationAdd), false, Some(body::<Vec<GoNacRelationshipRequest>>), Some(body::<Vec<AddActorRelationshipResult>>)),
    // The transaction's source-grant job checks the caller per edge.
    row_body(
        "/api/v0/acp/node/source-permission",
        POST,
        Dynamic,
        false,
        Some(body::<SourcePermission>),
        None,
    ),
    row_body("/api/v0/acp/node/disable", POST, Dynamic, false, None, Some(body::<()>)),
    row_body("/api/v0/acp/node/re-enable", POST, Dynamic, false, None, Some(body::<()>)),
    // Backup
    row_body("/api/v0/backup/export", POST, required(NodePermission::DocumentRead), false, Some(body::<ExportRequest>), None),
    row("/api/v0/backup/import", POST, required(NodePermission::DocumentUpdate), false),
    // Block
    row("/api/v0/block/verify-signature", GET, required(NodePermission::SignatureVerify), false),
    row_body("/api/v0/block/signed", GET, required(NodePermission::SignatureVerify), false, None, Some(body::<SignedBlockResponse>)),
    // Lens
    row_body("/api/v0/lens", POST, required(NodePermission::LensCreate), false, Some(body::<AddLensRequest>), Some(body::<SetMigrationResponse>)),
    row_body("/api/v0/lens", GET, required(NodePermission::LensList), false, None, Some(body::<serde_json::Value>)),
    // Batch signing
    row_body("/api/v0/batch/start", POST, Dynamic, false, None, Some(body::<()>)),
    row_body("/api/v0/batch/sign", POST, Dynamic, false, None, Some(body::<BatchSignResponse>)),
    // Views
    row_body("/api/v0/view", POST, required(NodePermission::ViewAdd), false, None, Some(body::<Vec<CollectionVersion>>)),
    row_body("/api/v0/view/refresh", POST, required(NodePermission::ViewRefresh), false, None, Some(body::<serde_json::Value>)),
    // Go has no view GC route.
    row_optional_body("/api/v0/views/gc", POST, required(NodePermission::ViewGc), false, body::<ViewNamesRequest>),
    // Encrypted indexes (global list)
    row("/api/v0/encrypted-indexes", GET, required(NodePermission::EncryptedIndexListAll), false),
    // Utility
    row_body("/api/v0/debug/dump", GET, required(NodePermission::DocumentRead), false, None, Some(body::<Vec<String>>)),
    row("/api/v0/purge", POST, required(NodePermission::DocumentUpdate), false),
    row_body("/api/v0/node/options", GET, required(NodePermission::P2pPeerInfo), false, None, Some(body::<Map<String, Value>>)),
    row_body("/api/v0/node/identity", GET, required(NodePermission::P2pPeerConnect), false, None, Some(body::<NodeIdentityResponse>)),
];

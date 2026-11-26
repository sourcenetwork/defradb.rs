//! The routes the server serves, one row per method.
//!
//! Kept out of `route_permissions.rs` so the table stays one line per route:
//! formatting it as a function call explodes each row into six lines, and a
//! reviewer needs to be able to scan it for a missing route. `cargo fmt` skips
//! the table and formats everything else.

use axum::http::Method;

use crate::route_permissions::RoutePermission;
use crate::router::NodePermission;

use RoutePermission::{Dynamic, Exempt, IdentityOnly};

/// One route the server serves, and what the auth middleware needs to know
/// about it.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Whether the handler rejects a caller it cannot identify, whatever the
    /// permission decides. `Dynamic` means the handler chooses, not that it
    /// accepts an anonymous call, so the security block is mandatory here even
    /// where the derived one is optional.
    pub caller_required: bool,
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
        caller_required: false,
    }
}

/// A row whose handler needs an authenticated caller.
const fn row_caller(
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
        caller_required: true,
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
    row("/api/v0/version", GET, Exempt, false),
    row("/api/v0/graphql/ws", ANY, Exempt, false),
    row("/api/v0/batch/verify", POST, Exempt, false),
    // GraphQL
    row("/api/v0/graphql/client-sign", GET, required(NodePermission::DocumentUpdate), false),
    // GET is always a read operation; POST determines permission by parsing
    // the query in the handler.
    row("/api/v0/graphql", GET, required(NodePermission::DocumentRead), false),
    row("/api/v0/graphql", POST, Dynamic, false),
    row("/api/v0/ccip", POST, Dynamic, false),
    row("/api/v0/ccip/{sender}/{data}", GET, Dynamic, false),
    row("/api/v0/events", GET, Dynamic, false),
    row("/api/v0/actions", GET, required(NodePermission::ActionList), false),
    row("/api/v0/schema", GET, required(NodePermission::CollectionGet), false),
    // Transactions (permissions enforced per-operation within tx)
    row("/api/v0/tx", POST, IdentityOnly, false),
    row("/api/v0/tx/{id}", GET, IdentityOnly, false),
    row("/api/v0/tx/{id}", POST, IdentityOnly, false),
    row("/api/v0/tx/{id}", DELETE, IdentityOnly, false),
    // Collections
    row("/api/v0/collections", GET, required(NodePermission::CollectionGet), false),
    row("/api/v0/collections", POST, required(NodePermission::CollectionPatch), false),
    row("/api/v0/collections", PATCH, required(NodePermission::CollectionPatch), false),
    // Dropping collections by name. This fell to the read default while
    // `delete_collections_by_names` enforced `CollectionPatch` itself, so the
    // outer gate was weaker than the handler.
    row("/api/v0/collections", DELETE, required(NodePermission::CollectionPatch), false),
    row("/api/v0/collections/default", POST, required(NodePermission::CollectionPatch), false),
    row("/api/v0/collections/versions", DELETE, required(NodePermission::CollectionPatch), false),
    row("/api/v0/collections/migrations", POST, required(NodePermission::MigrationSet), false),
    row("/api/v0/collections/indexes", GET, required(NodePermission::IndexList), false),
    row("/api/v0/collections/{name}", GET, required(NodePermission::CollectionGet), false),
    // POST and PATCH here are Go's filtered document operations, not
    // collection ones, so they are document permissions. Dropping a collection
    // is `DELETE /collections?name=...`, which keeps `CollectionPatch`.
    row("/api/v0/collections/{name}", POST, required(NodePermission::DocumentUpdate), true),
    row("/api/v0/collections/{name}", PATCH, required(NodePermission::DocumentUpdate), true),
    row("/api/v0/collections/{name}", DELETE, required(NodePermission::DocumentDelete), true),
    row("/api/v0/collections/{name}/describe", GET, required(NodePermission::CollectionGet), false),
    row("/api/v0/collections/{name}/exists", GET, required(NodePermission::CollectionGet), false),
    row("/api/v0/collections/{name}/truncate", DELETE, required(NodePermission::CollectionTruncate), true),
    // A purge is checked against the node permission of a truncate.
    row("/api/v0/collections/{name}/documents/purge", DELETE, required(NodePermission::CollectionTruncate), false),
    row("/api/v0/collections/{name}/document/{docID}", GET, required(NodePermission::DocumentRead), true),
    row("/api/v0/collections/{name}/document/{docID}", PATCH, required(NodePermission::DocumentUpdate), true),
    row("/api/v0/collections/{name}/document/{docID}", DELETE, required(NodePermission::DocumentDelete), true),
    row("/api/v0/collections/{name}/indexes", GET, required(NodePermission::IndexList), true),
    row("/api/v0/collections/{name}/indexes", POST, required(NodePermission::IndexCreate), true),
    row("/api/v0/collections/{name}/indexes/{index}", DELETE, required(NodePermission::IndexDelete), true),
    // The edge checks `get-collection` first and leaves the encrypted index
    // permission to the operation, so it comes after the collection on every
    // server, as in Go.
    row("/api/v0/collections/{name}/encrypted-indexes", GET, IdentityOnly, true),
    row("/api/v0/collections/{name}/encrypted-indexes", POST, IdentityOnly, true),
    row("/api/v0/collections/{name}/encrypted-indexes/{field}", DELETE, IdentityOnly, true),
    // P2P
    row("/api/v0/p2p/info", GET, required(NodePermission::P2pPeerInfo), false),
    row("/api/v0/p2p/sync/status", GET, required(NodePermission::P2pPeerInfo), false),
    row("/api/v0/p2p/shareable-address", GET, required(NodePermission::P2pPeerInfo), false),
    row("/api/v0/p2p/active-peers", GET, required(NodePermission::P2pPeerActive), false),
    row("/api/v0/p2p/connect", POST, required(NodePermission::P2pPeerConnect), false),
    row("/api/v0/p2p/disconnect", POST, required(NodePermission::P2pPeerDisconnect), false),
    row("/api/v0/p2p/peers/{peer_id}", GET, required(NodePermission::P2pPeerInfo), false),
    row("/api/v0/p2p/replicators", GET, required(NodePermission::P2pReplicatorList), false),
    row("/api/v0/p2p/replicators", POST, required(NodePermission::P2pReplicatorAdd), false),
    row("/api/v0/p2p/replicators", DELETE, required(NodePermission::P2pReplicatorDelete), false),
    row("/api/v0/p2p/collections", GET, required(NodePermission::P2pCollectionList), false),
    row("/api/v0/p2p/collections", POST, required(NodePermission::P2pCollectionAdd), false),
    row("/api/v0/p2p/collections", DELETE, required(NodePermission::P2pCollectionDelete), false),
    row("/api/v0/p2p/collections/sync-branchable", POST, required(NodePermission::P2pSyncBranchableCollection), false),
    row("/api/v0/p2p/collections/sync-versions", POST, required(NodePermission::P2pSyncCollectionVersions), false),
    row("/api/v0/p2p/documents", GET, required(NodePermission::P2pDocumentList), false),
    row("/api/v0/p2p/documents", POST, required(NodePermission::P2pDocumentAdd), false),
    row("/api/v0/p2p/documents", DELETE, required(NodePermission::P2pDocumentDelete), false),
    row("/api/v0/p2p/documents/sync", POST, required(NodePermission::P2pSyncDocuments), false),
    // Management relay: this node connects to and commands the target peer, so
    // it reuses the peer-connect permission to prevent an open relay.
    row("/api/v0/p2p/manage", POST, required(NodePermission::P2pPeerConnect), false),
    row("/api/v0/p2p/manage/query", POST, required(NodePermission::P2pPeerConnect), false),
    // ACP (Document Access Control)
    row("/api/v0/acp/status", GET, required(NodePermission::DacStatus), false),
    row("/api/v0/acp/document/policy", POST, required(NodePermission::DacPolicyAdd), false),
    row("/api/v0/acp/policy", GET, required(NodePermission::DacStatus), false),
    row("/api/v0/acp/policy/{id}", GET, required(NodePermission::DacStatus), false),
    row("/api/v0/acp/document/decide", POST, required(NodePermission::DacStatus), false),
    // Reads the documents in the caller's transaction, as a query does.
    row("/api/v0/acp/document/is-owner", POST, required(NodePermission::DocumentRead), false),
    row("/api/v0/acp/document/relationship", GET, Dynamic, false),
    row("/api/v0/acp/document/relationship", POST, required(NodePermission::DacRelationAdd), false),
    row("/api/v0/acp/document/relationship", DELETE, required(NodePermission::DacRelationDelete), false),
    // Listing is gated by the document's owner or managers in the core.
    row("/api/v0/acp/document/relationships", GET, Dynamic, false),
    row("/api/v0/acp/document/relationships", POST, required(NodePermission::DacRelationAdd), false),
    // ACP Node (NAC via Go-compatible /acp/node/* routes)
    row("/api/v0/acp/node/status", GET, required(NodePermission::NacStatus), false),
    // The subject itself, an admin or a `NacStatus` holder: decided in the core.
    row("/api/v0/acp/node/explain", GET, Dynamic, false),
    row_caller("/api/v0/acp/node/enable", POST, Dynamic, false),
    row("/api/v0/acp/node/relationship", POST, required(NodePermission::NacRelationAdd), false),
    row("/api/v0/acp/node/relationship", DELETE, required(NodePermission::NacRelationDelete), false),
    row("/api/v0/acp/node/relationships", POST, required(NodePermission::NacRelationAdd), false),
    // The transaction's source-grant job checks the caller per edge.
    row_caller("/api/v0/acp/node/source-permission", POST, Dynamic, false),
    row("/api/v0/acp/node/disable", POST, Dynamic, false),
    row("/api/v0/acp/node/re-enable", POST, Dynamic, false),
    // Backup
    row("/api/v0/backup/export", POST, required(NodePermission::DocumentRead), false),
    row("/api/v0/backup/import", POST, required(NodePermission::DocumentUpdate), false),
    // Block
    row("/api/v0/block/verify-signature", GET, required(NodePermission::SignatureVerify), false),
    row("/api/v0/block/signed", GET, required(NodePermission::SignatureVerify), false),
    // Lens
    row("/api/v0/lens", POST, required(NodePermission::LensCreate), false),
    row("/api/v0/lens", GET, required(NodePermission::LensList), false),
    // Batch signing
    row("/api/v0/batch/start", POST, Dynamic, false),
    row("/api/v0/batch/sign", POST, Dynamic, false),
    // Views
    row("/api/v0/view", POST, required(NodePermission::ViewAdd), false),
    row("/api/v0/view/refresh", POST, required(NodePermission::ViewRefresh), false),
    // Go has no view GC route.
    row("/api/v0/views/gc", POST, required(NodePermission::ViewGc), false),
    // Encrypted indexes (global list)
    row("/api/v0/encrypted-indexes", GET, required(NodePermission::EncryptedIndexListAll), false),
    // Utility
    row("/api/v0/debug/dump", GET, required(NodePermission::DocumentRead), false),
    row("/api/v0/purge", POST, required(NodePermission::DocumentUpdate), false),
    row("/api/v0/node/options", GET, required(NodePermission::P2pPeerInfo), false),
    row("/api/v0/node/identity", GET, required(NodePermission::P2pPeerConnect), false),
];

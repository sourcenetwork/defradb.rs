//! The route permission table, which the auth middleware consults for every
//! request.
//!
//! The table is keyed on axum's `MatchedPath`, so a route registered at a
//! second path does not inherit the first path's entry. A missing entry is not
//! a compile error and not a 404: it falls to the safe default and the route is
//! enforced as `DocumentRead`, which on a privileged route is a downgrade.
//! `ROUTES` is the one list, and `every_registered_route_has_a_row` in the
//! crate's own tests scans `router/routes.rs` for a route registered without a
//! row. This file proves each row folds onto its `/api/v0` key under every
//! mount, and that every row resolves in the router.
//!
mod common;

use std::collections::BTreeSet;

use axum::body::Body;
use axum::http::Method;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use defra_http::route_permissions::{
    reads_collection_first, route_permission, RoutePermission, ROUTES,
};
use defra_http::router::NodePermission;

#[test]
fn exempt_routes() {
    assert_eq!(
        route_permission("/health-check", &Method::GET),
        RoutePermission::Exempt
    );
    assert_eq!(
        route_permission("/openapi.json", &Method::GET),
        RoutePermission::Exempt
    );
    assert_eq!(
        route_permission("/api/v0/version", &Method::GET),
        RoutePermission::Exempt
    );
    assert_eq!(
        route_permission("/api/v0/graphql/ws", &Method::GET),
        RoutePermission::Exempt
    );
    assert_eq!(
        route_permission("/api/v0/batch/verify", &Method::POST),
        RoutePermission::Exempt
    );
}

#[test]
fn identity_only_routes() {
    assert_eq!(
        route_permission("/api/v0/tx", &Method::POST),
        RoutePermission::IdentityOnly
    );
    assert_eq!(
        route_permission("/api/v0/tx/{id}", &Method::GET),
        RoutePermission::IdentityOnly
    );
    assert_eq!(
        route_permission("/api/v0/tx/{id}", &Method::POST),
        RoutePermission::IdentityOnly
    );
    assert_eq!(
        route_permission("/api/v0/tx/{id}", &Method::DELETE),
        RoutePermission::IdentityOnly
    );
}

#[test]
fn dynamic_routes() {
    assert_eq!(
        route_permission("/api/v0/ccip", &Method::POST),
        RoutePermission::Dynamic
    );
    assert_eq!(
        route_permission("/api/v0/ccip/{sender}/{data}", &Method::GET),
        RoutePermission::Dynamic
    );
    assert_eq!(
        route_permission("/api/v0/graphql", &Method::POST),
        RoutePermission::Dynamic
    );
    assert_eq!(
        route_permission("/api/v0/acp/node/enable", &Method::POST),
        RoutePermission::Dynamic
    );
    assert_eq!(
        route_permission("/api/v0/acp/node/disable", &Method::POST),
        RoutePermission::Dynamic
    );
    assert_eq!(
        route_permission("/api/v0/acp/node/re-enable", &Method::POST),
        RoutePermission::Dynamic
    );
    assert_eq!(
        route_permission("/api/v0/batch/start", &Method::POST),
        RoutePermission::Dynamic
    );
    assert_eq!(
        route_permission("/api/v0/batch/sign", &Method::POST),
        RoutePermission::Dynamic
    );
}

#[test]
fn collection_routes() {
    assert_eq!(
        route_permission("/api/v0/collections", &Method::GET),
        RoutePermission::Required(NodePermission::CollectionGet)
    );
    assert_eq!(
        route_permission("/api/v0/collections", &Method::PATCH),
        RoutePermission::Required(NodePermission::CollectionPatch)
    );
    assert_eq!(
        route_permission("/api/v0/collections/{name}", &Method::POST),
        RoutePermission::Required(NodePermission::DocumentUpdate)
    );
    assert_eq!(
        route_permission("/api/v0/collections/{name}", &Method::DELETE),
        RoutePermission::Required(NodePermission::DocumentDelete)
    );
}

#[test]
fn document_routes() {
    assert_eq!(
        route_permission("/api/v0/collections/{name}/document/{docID}", &Method::GET),
        RoutePermission::Required(NodePermission::DocumentRead)
    );
    assert_eq!(
        route_permission(
            "/api/v0/collections/{name}/document/{docID}",
            &Method::PATCH,
        ),
        RoutePermission::Required(NodePermission::DocumentUpdate)
    );
    assert_eq!(
        route_permission(
            "/api/v0/collections/{name}/document/{docID}",
            &Method::DELETE,
        ),
        RoutePermission::Required(NodePermission::DocumentDelete)
    );
}

#[test]
fn p2p_routes() {
    assert_eq!(
        route_permission("/api/v0/p2p/info", &Method::GET),
        RoutePermission::Required(NodePermission::P2pPeerInfo)
    );
    assert_eq!(
        route_permission("/api/v0/p2p/shareable-address", &Method::GET),
        RoutePermission::Required(NodePermission::P2pPeerInfo)
    );
    assert_eq!(
        route_permission("/api/v0/p2p/disconnect", &Method::POST),
        RoutePermission::Required(NodePermission::P2pPeerDisconnect)
    );
    assert_eq!(
        route_permission("/api/v0/p2p/replicators", &Method::GET),
        RoutePermission::Required(NodePermission::P2pReplicatorList)
    );
    assert_eq!(
        route_permission("/api/v0/p2p/replicators", &Method::POST),
        RoutePermission::Required(NodePermission::P2pReplicatorAdd)
    );
    assert_eq!(
        route_permission("/api/v0/p2p/replicators", &Method::DELETE),
        RoutePermission::Required(NodePermission::P2pReplicatorDelete)
    );
    assert_eq!(
        route_permission("/api/v0/p2p/documents/sync", &Method::POST),
        RoutePermission::Required(NodePermission::P2pSyncDocuments)
    );
    assert_eq!(
        route_permission("/api/v0/p2p/manage", &Method::POST),
        RoutePermission::Required(NodePermission::P2pPeerConnect)
    );
    assert_eq!(
        route_permission("/api/v0/p2p/manage/query", &Method::POST),
        RoutePermission::Required(NodePermission::P2pPeerConnect)
    );
}

#[test]
fn acp_status_route_uses_dac_status_permission() {
    // Regression test for #758: the route was missing from the
    // permission table and falling through to DocumentRead, which
    // over-restricted callers that only had DacStatus.
    assert_eq!(
        route_permission("/api/v0/acp/status", &Method::GET),
        RoutePermission::Required(NodePermission::DacStatus)
    );
}

#[test]
fn acp_document_decide_route_uses_dac_status_permission() {
    assert_eq!(
        route_permission("/api/v0/acp/document/decide", &Method::POST),
        RoutePermission::Required(NodePermission::DacStatus)
    );
}

#[test]
fn unknown_route_gets_safe_default() {
    assert_eq!(
        route_permission("/api/v0/unknown", &Method::GET),
        RoutePermission::Required(NodePermission::DocumentRead)
    );
}

#[test]
fn all_registered_routes_return_expected_permission() {
    // `ROUTES` is the list; this proves every mount of it folds onto the
    // `/api/v0` row, which is what `normalize_api_version` exists for. That the
    // rows are routes the router serves is
    // `every_listed_route_resolves_in_the_router` below.
    for row in ROUTES {
        let methods = match &row.method {
            Some(method) => vec![method.clone()],
            // The one `any()` route: prove the wildcard rather than skip it.
            None => vec![Method::GET, Method::DELETE],
        };
        for method in methods {
            for candidate in mount_forms(row.path) {
                let actual = route_permission(&candidate, &method);
                assert_eq!(
                    actual, row.permission,
                    "Mismatch for {method} {candidate}: expected {:?}, got {:?}",
                    row.permission, actual
                );
            }
        }
    }
}

/// The same route as every prefix in `go_paths::API_PREFIXES` mounts it.
fn mount_forms(v0_path: &str) -> Vec<String> {
    let Some(suffix) = v0_path.strip_prefix("/api/v0") else {
        return vec![v0_path.to_string()];
    };
    defra_http::go_paths::API_PREFIXES
        .iter()
        .map(|prefix| format!("{prefix}{suffix}"))
        .collect()
}

#[test]
fn every_mount_form_uses_the_same_permission_as_v0() {
    for (v0_path, method) in [
        ("/api/v0/version", Method::GET),
        ("/api/v0/graphql", Method::POST),
        ("/api/v0/collections/{name}", Method::POST),
        ("/api/v0/p2p/replicators", Method::DELETE),
        ("/api/v0/acp/node/disable", Method::POST),
        ("/api/v0/backup/export", Method::POST),
        ("/api/v0/block/signed", Method::GET),
    ] {
        let expected = route_permission(v0_path, &method);
        for candidate in mount_forms(v0_path) {
            assert_eq!(
                route_permission(&candidate, &method),
                expected,
                "{candidate} disagrees with {v0_path} for {method}"
            );
        }
    }
}

/// A path that only looks versioned must not be rewritten into the table.
#[test]
fn near_miss_version_segments_are_not_folded() {
    for path in ["/api/v10/collections", "/api/v2/collections"] {
        assert_eq!(
            route_permission(path, &Method::GET),
            RoutePermission::Required(NodePermission::DocumentRead),
            "{path} should fall to the safe default"
        );
    }
}

/// Every row resolves in the router.
///
/// A row that does not is a route the generated OpenAPI document would
/// advertise and the server would answer 404 for. `TRACE` keeps the probe
/// independent of what a handler does: a registered path answers 405 for a
/// method it does not take, and only an unregistered one falls through to 404.
#[test]
fn every_listed_route_resolves_in_the_router() {
    edge::testing::edge()
        .block_on(async {
            let app = common::router();
            for row in ROUTES {
                let path = common::concrete_path(row.path);
                for candidate in mount_forms(&path) {
                    let response = app
                        .clone()
                        .oneshot(
                            Request::builder()
                                .method(Method::TRACE)
                                .uri(&candidate)
                                .body(Body::empty())
                                .unwrap(),
                        )
                        .await
                        .expect("router should respond");
                    assert_ne!(
                        response.status(),
                        StatusCode::NOT_FOUND,
                        "{candidate} is listed in ROUTES and not served"
                    );
                }
            }
        })
        .unwrap();
}

/// The permission every route was reviewed with, as an independent oracle.
///
/// `ROUTES` is the runtime authority; this list is the review record. A row
/// compared against the lookup that reads it proves nothing, so the values are
/// written out here: changing a permission, or adding or removing a row without
/// updating this list, fails, and the diff shows the change. The values below
/// were checked against the `match` this table replaced, row by row and mount by
/// mount, when the table landed.
const REVIEWED: &[(&str, &str, &str)] = &[
    ("/health-check", "GET", "exempt"),
    ("/openapi.json", "GET", "exempt"),
    ("/openapi.rs.json", "GET", "exempt"),
    ("/api/v0/version", "GET", "exempt"),
    ("/api/v0/graphql/ws", "ANY", "exempt"),
    ("/api/v0/batch/verify", "POST", "exempt"),
    ("/api/v0/graphql/client-sign", "GET", "DocumentUpdate"),
    ("/api/v0/graphql", "GET", "DocumentRead"),
    ("/api/v0/graphql", "POST", "dynamic"),
    ("/api/v0/ccip", "POST", "dynamic"),
    ("/api/v0/ccip/{sender}/{data}", "GET", "dynamic"),
    ("/api/v0/events", "GET", "dynamic"),
    ("/api/v0/actions", "GET", "ActionList"),
    ("/api/v0/schema", "GET", "CollectionGet"),
    ("/api/v0/tx", "POST", "identity-only"),
    ("/api/v0/tx/{id}", "GET", "identity-only"),
    ("/api/v0/tx/{id}", "POST", "identity-only"),
    ("/api/v0/tx/{id}", "DELETE", "identity-only"),
    ("/api/v0/collections", "GET", "CollectionGet"),
    ("/api/v0/collections", "POST", "CollectionPatch"),
    ("/api/v0/collections", "PATCH", "CollectionPatch"),
    ("/api/v0/collections", "DELETE", "CollectionPatch"),
    ("/api/v0/collections/default", "POST", "CollectionPatch"),
    ("/api/v0/collections/versions", "DELETE", "CollectionPatch"),
    ("/api/v0/collections/migrations", "POST", "MigrationSet"),
    ("/api/v0/collections/indexes", "GET", "IndexList"),
    ("/api/v0/collections/{name}", "GET", "CollectionGet"),
    ("/api/v0/collections/{name}", "POST", "DocumentUpdate"),
    ("/api/v0/collections/{name}", "PATCH", "DocumentUpdate"),
    ("/api/v0/collections/{name}", "DELETE", "DocumentDelete"),
    (
        "/api/v0/collections/{name}/describe",
        "GET",
        "CollectionGet",
    ),
    ("/api/v0/collections/{name}/exists", "GET", "CollectionGet"),
    (
        "/api/v0/collections/{name}/truncate",
        "DELETE",
        "CollectionTruncate",
    ),
    (
        "/api/v0/collections/{name}/documents/purge",
        "DELETE",
        "CollectionTruncate",
    ),
    (
        "/api/v0/collections/{name}/document/{docID}",
        "GET",
        "DocumentRead",
    ),
    (
        "/api/v0/collections/{name}/document/{docID}",
        "PATCH",
        "DocumentUpdate",
    ),
    (
        "/api/v0/collections/{name}/document/{docID}",
        "DELETE",
        "DocumentDelete",
    ),
    ("/api/v0/collections/{name}/indexes", "GET", "IndexList"),
    ("/api/v0/collections/{name}/indexes", "POST", "IndexCreate"),
    (
        "/api/v0/collections/{name}/indexes/{index}",
        "DELETE",
        "IndexDelete",
    ),
    (
        "/api/v0/collections/{name}/encrypted-indexes",
        "GET",
        "identity-only",
    ),
    (
        "/api/v0/collections/{name}/encrypted-indexes",
        "POST",
        "identity-only",
    ),
    (
        "/api/v0/collections/{name}/encrypted-indexes/{field}",
        "DELETE",
        "identity-only",
    ),
    ("/api/v0/p2p/info", "GET", "P2pPeerInfo"),
    ("/api/v0/p2p/sync/status", "GET", "P2pPeerInfo"),
    ("/api/v0/p2p/shareable-address", "GET", "P2pPeerInfo"),
    ("/api/v0/p2p/active-peers", "GET", "P2pPeerActive"),
    ("/api/v0/p2p/connect", "POST", "P2pPeerConnect"),
    ("/api/v0/p2p/disconnect", "POST", "P2pPeerDisconnect"),
    ("/api/v0/p2p/peers/{peer_id}", "GET", "P2pPeerInfo"),
    ("/api/v0/p2p/replicators", "GET", "P2pReplicatorList"),
    ("/api/v0/p2p/replicators", "POST", "P2pReplicatorAdd"),
    ("/api/v0/p2p/replicators", "DELETE", "P2pReplicatorDelete"),
    ("/api/v0/p2p/collections", "GET", "P2pCollectionList"),
    ("/api/v0/p2p/collections", "POST", "P2pCollectionAdd"),
    ("/api/v0/p2p/collections", "DELETE", "P2pCollectionDelete"),
    (
        "/api/v0/p2p/collections/sync-branchable",
        "POST",
        "P2pSyncBranchableCollection",
    ),
    (
        "/api/v0/p2p/collections/sync-versions",
        "POST",
        "P2pSyncCollectionVersions",
    ),
    ("/api/v0/p2p/documents", "GET", "P2pDocumentList"),
    ("/api/v0/p2p/documents", "POST", "P2pDocumentAdd"),
    ("/api/v0/p2p/documents", "DELETE", "P2pDocumentDelete"),
    ("/api/v0/p2p/documents/sync", "POST", "P2pSyncDocuments"),
    ("/api/v0/p2p/manage", "POST", "P2pPeerConnect"),
    ("/api/v0/p2p/manage/query", "POST", "P2pPeerConnect"),
    ("/api/v0/acp/status", "GET", "DacStatus"),
    ("/api/v0/acp/document/policy", "POST", "DacPolicyAdd"),
    ("/api/v0/acp/policy", "GET", "DacStatus"),
    ("/api/v0/acp/policy/{id}", "GET", "DacStatus"),
    ("/api/v0/acp/document/decide", "POST", "DacStatus"),
    ("/api/v0/acp/document/is-owner", "POST", "DocumentRead"),
    ("/api/v0/acp/document/relationship", "GET", "dynamic"),
    (
        "/api/v0/acp/document/relationship",
        "POST",
        "DacRelationAdd",
    ),
    (
        "/api/v0/acp/document/relationship",
        "DELETE",
        "DacRelationDelete",
    ),
    ("/api/v0/acp/document/relationships", "GET", "dynamic"),
    (
        "/api/v0/acp/document/relationships",
        "POST",
        "DacRelationAdd",
    ),
    ("/api/v0/acp/node/status", "GET", "NacStatus"),
    ("/api/v0/acp/node/explain", "GET", "dynamic"),
    ("/api/v0/acp/node/enable", "POST", "dynamic"),
    ("/api/v0/acp/node/relationship", "POST", "NacRelationAdd"),
    (
        "/api/v0/acp/node/relationship",
        "DELETE",
        "NacRelationDelete",
    ),
    ("/api/v0/acp/node/relationships", "POST", "NacRelationAdd"),
    ("/api/v0/acp/node/source-permission", "POST", "dynamic"),
    ("/api/v0/acp/node/disable", "POST", "dynamic"),
    ("/api/v0/acp/node/re-enable", "POST", "dynamic"),
    ("/api/v0/backup/export", "POST", "DocumentRead"),
    ("/api/v0/backup/import", "POST", "DocumentUpdate"),
    ("/api/v0/block/verify-signature", "GET", "SignatureVerify"),
    ("/api/v0/block/signed", "GET", "SignatureVerify"),
    ("/api/v0/lens", "POST", "LensCreate"),
    ("/api/v0/lens", "GET", "LensList"),
    ("/api/v0/batch/start", "POST", "dynamic"),
    ("/api/v0/batch/sign", "POST", "dynamic"),
    ("/api/v0/view", "POST", "ViewAdd"),
    ("/api/v0/view/refresh", "POST", "ViewRefresh"),
    ("/api/v0/views/gc", "POST", "ViewGc"),
    ("/api/v0/encrypted-indexes", "GET", "EncryptedIndexListAll"),
    ("/api/v0/debug/dump", "GET", "DocumentRead"),
    ("/api/v0/purge", "POST", "DocumentUpdate"),
    ("/api/v0/node/options", "GET", "P2pPeerInfo"),
    ("/api/v0/node/identity", "GET", "P2pPeerConnect"),
];

/// A permission as `REVIEWED` spells it.
fn reviewed_permission(name: &str) -> RoutePermission {
    match name {
        "exempt" => RoutePermission::Exempt,
        "dynamic" => RoutePermission::Dynamic,
        "identity-only" => RoutePermission::IdentityOnly,
        node => RoutePermission::Required(
            serde_json::from_str::<NodePermission>(&format!("\"{node}\""))
                .expect("a node permission name"),
        ),
    }
}

/// Every row's permission against the reviewed value, and every reviewed value
/// against a row: the two lists have to be the same set.
#[test]
fn the_table_matches_the_reviewed_permissions() {
    let mut reviewed: BTreeSet<(&str, Option<Method>)> = BTreeSet::new();
    for (path, method, permission) in REVIEWED {
        let method = if *method == "ANY" {
            None
        } else {
            Some(Method::from_bytes(method.as_bytes()).expect("a method"))
        };
        let actual = match &method {
            Some(method) => route_permission(path, method),
            None => route_permission(path, &Method::GET),
        };
        assert_eq!(actual, reviewed_permission(permission), "{method:?} {path}");
        assert!(
            reviewed.insert((path, method.clone())),
            "{method:?} {path} is reviewed twice"
        );
    }

    let rows: BTreeSet<(&str, Option<Method>)> = ROUTES
        .iter()
        .map(|row| (row.path, row.method.clone()))
        .collect();
    assert_eq!(
        reviewed, rows,
        "every row has exactly one reviewed permission, and every reviewed \
         permission is a row"
    );
}

/// Axum serves a `HEAD` from the route's `get` registration, with the body
/// removed, and the method reaches the edge unchanged. An implicit HEAD is
/// therefore authorized as the GET it runs — before this held, every HEAD fell
/// to the read default and a public health probe answered 401 under NAC.
#[test]
fn an_implicit_head_takes_the_get_rows_permission() {
    for mount in mount_forms("/api/v0/version") {
        assert_eq!(
            route_permission(&mount, &Method::HEAD),
            RoutePermission::Exempt,
            "HEAD {mount}"
        );
    }
    assert_eq!(
        route_permission("/health-check", &Method::HEAD),
        RoutePermission::Exempt
    );
    assert_eq!(
        route_permission("/api/v0/collections/{name}", &Method::HEAD),
        RoutePermission::Required(NodePermission::CollectionGet),
        "HEAD reads a collection, so it takes the GET row, not the read default"
    );
    assert_eq!(
        route_permission("/api/v0/graphql", &Method::HEAD),
        RoutePermission::Required(NodePermission::DocumentRead),
        "HEAD takes /graphql's GET row, not its POST row"
    );
    assert!(
        reads_collection_first("/api/v0/collections/{name}/document/{docID}", &Method::HEAD),
        "the collection-first check follows the GET row for HEAD"
    );

    // The `match` this table replaced gave these two paths' HEAD the
    // `DacRelationAdd` its per-path `_` arm fell to. The GET handler axum runs
    // for HEAD is `Dynamic`, so HEAD now answers what actually executes.
    for path in [
        "/api/v0/acp/document/relationship",
        "/api/v0/acp/document/relationships",
    ] {
        assert_eq!(
            route_permission(path, &Method::HEAD),
            RoutePermission::Dynamic,
            "HEAD {path}"
        );
    }
}

/// The fallback is for HEAD alone: a method the router does not register still
/// lands on the safe default rather than inheriting the path's GET.
#[test]
fn an_unregistered_method_keeps_the_safe_default() {
    assert_eq!(
        route_permission("/api/v0/collections", &Method::PUT),
        RoutePermission::Required(NodePermission::DocumentRead)
    );
}

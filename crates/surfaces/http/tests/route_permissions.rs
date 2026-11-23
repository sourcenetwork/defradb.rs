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

use axum::body::Body;
use axum::http::Method;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use defra_http::route_permissions::{route_permission, RoutePermission, ROUTES};
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

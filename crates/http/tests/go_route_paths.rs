//! Go's wire paths for policy registration and views must resolve on Rust, and
//! the Rust aliases that used to sit beside them must not.
//!
//! Go's client posts policies to `/acp/document/policy` and views to `/view`.
//! Each route needs its own permission entry or it silently falls to the safe
//! default.

mod common;
use common::*;

// ---------------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------------

/// The constants the router, the CLI and Go's client all have to agree on must
/// actually resolve, under every API version the server mounts. Without this
/// the shared constant is just a fourth copy of the string.
#[tokio::test]
async fn every_declared_go_path_resolves_under_every_api_version() {
    for prefix in defra_http::go_paths::API_PREFIXES {
        for path in defra_http::go_paths::ALL {
            let (status, body) = Call::post(&format!("{prefix}{path}"))
                .json("{}")
                .send()
                .await;
            assert_ne!(
                status,
                StatusCode::NOT_FOUND,
                "{prefix}{path} is declared but not served: {body}"
            );
        }
    }
}

/// Go's client posts to `acp/document/policy`. Rust 404'd there.
#[tokio::test]
async fn gos_policy_path_resolves() {
    for version in ["v0", "v1"] {
        let (status, body) = Call::post(&format!("/api/{version}/acp/document/policy"))
            .body(POLICY)
            .authenticated()
            .send()
            .await;
        assert_ne!(status, StatusCode::NOT_FOUND, "{version}: {body}");
    }
}

/// Go's client posts views to `view`, not `views`.
#[tokio::test]
async fn gos_view_paths_resolve() {
    for version in ["v0", "v1"] {
        for (path, body) in [
            ("view", r#"{"Query":"q","SDL":"s"}"#),
            ("view/refresh", "{}"),
        ] {
            let (status, response) = Call::post(&format!("/api/{version}/{path}"))
                .json(body)
                .send()
                .await;
            assert_ne!(
                status,
                StatusCode::NOT_FOUND,
                "{version}/{path}: {response}"
            );
        }
    }
}

/// Rust-only routes keep their paths.
#[tokio::test]
async fn the_rust_only_paths_still_resolve() {
    let calls = [
        Call::post("/api/v0/views/gc").json("{}"),
        Call::post("/api/v0/acp/policy")
            .method(Method::GET)
            .authenticated(),
    ];
    for call in calls {
        let (status, body) = call.send().await;
        assert_ne!(status, StatusCode::NOT_FOUND, "{}: {body}", call.path);
    }
}

/// Paths that only aliased a Go route are gone.
#[tokio::test]
async fn the_removed_aliases_are_not_served() {
    let calls = [
        Call::post("/api/v0/acp/policy")
            .body(POLICY)
            .authenticated(),
        Call::post("/api/v0/views").json(r#"{"Query":"q","SDL":"s"}"#),
        Call::post("/api/v0/views/refresh").json("{}"),
        Call::post("/api/v0/index").json("{}"),
        Call::post("/api/v0/lens/set").json("{}"),
        Call::post("/api/v0/schema").body("type A { a: String }"),
        Call::post("/api/v0/nac/status").method(Method::GET),
        Call::post("/api/v0/nac/admin").json("{}"),
        Call::post("/api/v0/p2p/replicator").method(Method::GET),
        Call::post("/api/v0/p2p/peers").method(Method::GET),
    ];
    for call in calls {
        let (status, body) = call.send().await;
        assert!(
            matches!(
                status,
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
            "{} is still served: {status} {body}",
            call.path
        );
    }
}

/// `/gc` is Rust's own route. Go has no `/view/gc`, and the Go mount must not
/// acquire Rust extras just because it shares a handler set.
#[tokio::test]
async fn the_go_view_mount_has_no_gc_route() {
    let (status, _) = Call::post("/api/v0/view/gc").json("{}").send().await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// axum panics at construction on overlapping routes, and this change mounts
/// one router at two prefixes and one handler at two paths. Every other test
/// builds a router, but only this one says that is the thing being checked.
#[tokio::test]
async fn the_router_builds() {
    let _ = router();
}

/// A method Go does not register answers 405.
#[tokio::test]
async fn a_method_mismatch_answers_405() {
    for path in ["/api/v0/acp/document/policy", "/api/v0/view/refresh"] {
        let (status, _) = Call::post(path).method(Method::PUT).send().await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{path}");
    }
}

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

/// Every Go path in this file, with the permission it carries.
const GO_PATHS: &[(&str, NodePermission)] = &[
    ("/api/v0/acp/document/policy", NodePermission::DacPolicyAdd),
    ("/api/v0/view", NodePermission::ViewAdd),
    ("/api/v0/view/refresh", NodePermission::ViewRefresh),
];

#[tokio::test]
async fn every_new_path_carries_its_own_permission() {
    for (go_path, expected) in GO_PATHS {
        assert_eq!(
            route_permission(go_path, &Method::POST),
            RoutePermission::Required(*expected),
            "{go_path}"
        );
    }
}

/// The guard for the trap this change could have shipped: a route registered
/// without a permission entry falls to `Required(DocumentRead)`, which would
/// let a read-only actor register a DAC policy.
#[tokio::test]
async fn no_new_path_falls_to_the_safe_default() {
    for (go_path, _) in GO_PATHS {
        assert_ne!(
            route_permission(go_path, &Method::POST),
            RoutePermission::Required(NodePermission::DocumentRead),
            "{go_path} is not in the permission table"
        );
    }
}

/// Go's client talks to `/api/v1`, so the v1 form has to resolve to the same
/// permission as the v0 form it is normalized onto.
#[tokio::test]
async fn both_api_versions_resolve_the_same_permission() {
    for (go_path, _) in GO_PATHS {
        let v1_path = go_path.replacen("/api/v0/", "/api/v1/", 1);
        assert_eq!(
            route_permission(&v1_path, &Method::POST),
            route_permission(go_path, &Method::POST),
            "{v1_path}"
        );
    }
}

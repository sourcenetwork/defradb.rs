//! The edge decides a collection handler route's two node permissions,
//! `get-collection` and then the route's own, in one NAC call, and its
//! refusal names the first one the caller is missing, as Go's
//! `CollectionMiddleware` followed by the handler's check does.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use axum::http::Request;
use defra_http::router::{NacStatus, NodeAcpOperations, PermissionCheck};
use defra_http::{MockNodeAcpOperations, MockQueryExecutor, Server};
use tower::ServiceExt;

use common::{Arc, NodePermission, StatusCode, TEST_HOST};

/// The mock NAC, counting how often each check entry point is called.
struct Counting {
    inner: MockNodeAcpOperations,
    single: AtomicUsize,
    batched: AtomicUsize,
}

#[async_trait]
impl NodeAcpOperations for Counting {
    async fn check_permission(
        &self,
        identity: &identity::Did,
        permission: NodePermission,
    ) -> Result<bool, String> {
        self.single.fetch_add(1, Ordering::SeqCst);
        self.inner.check_permission(identity, permission).await
    }

    async fn check_permissions(
        &self,
        identity: &identity::Did,
        permissions: &[NodePermission],
    ) -> PermissionCheck {
        self.batched.fetch_add(1, Ordering::SeqCst);
        self.inner.check_permissions(identity, permissions).await
    }

    async fn get_status(&self) -> NacStatus {
        self.inner.get_status().await
    }

    async fn enable(&self, owner: &identity::Did) -> Result<(), String> {
        self.inner.enable(owner).await
    }
}

/// Posts a document as a caller holding `grants`, returning the status, the
/// body and the calls the edge made: (batched, single).
async fn add_document(grants: &[NodePermission]) -> (StatusCode, String, usize, usize) {
    let (owner, _) = common::nac_owner();
    let (caller, token) = common::nac_owner();
    let mut inner = MockNodeAcpOperations::enabled_with_owner(owner);
    for permission in grants {
        inner = inner.with_grant(caller.clone(), *permission);
    }
    let nac = Arc::new(Counting {
        inner,
        single: AtomicUsize::new(0),
        batched: AtomicUsize::new(0),
    });
    let response = Server::new(MockQueryExecutor::new(), edge::testing::edge())
        .with_nac_arc(nac.clone())
        .router()
        .unwrap()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v0/collections/User")
                .header(HOST, TEST_HOST)
                .header(AUTHORIZATION, token)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"Shahzad"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        String::from_utf8_lossy(&body).into_owned(),
        nac.batched.load(Ordering::SeqCst),
        nac.single.load(Ordering::SeqCst),
    )
}

fn assert_refused(grants: &[NodePermission], permission: NodePermission) {
    let (status, body, batched, single) = edge::testing::edge()
        .block_on(add_document(grants))
        .unwrap();
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(
        body.contains(&format!("Permission: {}", permission.as_str())),
        "the refusal does not name {}: {body}",
        permission.as_str()
    );
    assert_eq!(
        (batched, single),
        (1, 0),
        "the edge made more than one call"
    );
}

#[test]
fn caller_holding_neither_is_refused_get_collection() {
    assert_refused(&[], NodePermission::CollectionGet);
}

#[test]
fn caller_holding_only_the_route_permission_is_refused_get_collection() {
    assert_refused(
        &[NodePermission::DocumentUpdate],
        NodePermission::CollectionGet,
    );
}

#[test]
fn caller_holding_only_get_collection_is_refused_the_route_permission() {
    assert_refused(
        &[NodePermission::CollectionGet],
        NodePermission::DocumentUpdate,
    );
}

#[test]
fn caller_holding_both_passes_the_edge() {
    let (status, body, _, _) = edge::testing::edge()
        .block_on(add_document(&[
            NodePermission::CollectionGet,
            NodePermission::DocumentUpdate,
        ]))
        .unwrap();
    assert!(
        !body.contains("not authorized to perform operation"),
        "{status}: {body}"
    );
}

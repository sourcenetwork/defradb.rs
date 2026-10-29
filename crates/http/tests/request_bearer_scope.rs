//! Same-identity HTTP requests must retain their own bearer through awaits.
use std::{sync::Arc, time::Duration};

use axum::{
    body::{to_bytes, Body},
    http::{
        header::{AUTHORIZATION, HOST},
        Request,
    },
    routing::get,
    Extension, Json, Router,
};
use defra_http::{router::AppStateBuilder, ExtractIdentity, MockQueryExecutor};
use identity::{new_token, Identity, RawIdentity};
use regolith::sync::Barrier;
use tower::ServiceExt;

async fn observe(
    identity: ExtractIdentity,
    Extension(barrier): Extension<Arc<Barrier>>,
) -> Json<Vec<Option<String>>> {
    let did = identity.did().unwrap().as_str();
    let before = defra_core::signing::get_request_bearer_token(did);
    barrier.wait().await;
    nuclei::task::yield_now().await;
    let after = defra_core::signing::get_request_bearer_token(did);
    Json(vec![before, after])
}

#[nuclei::test]
async fn concurrent_same_identity_requests_keep_distinct_bearers() {
    let identity = RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
    let did = identity.did().unwrap();
    let token = |lifetime| {
        String::from_utf8(
            new_token(
                &identity,
                Duration::from_secs(lifetime),
                Some("localhost:9181".into()),
                None,
            )
            .unwrap(),
        )
        .unwrap()
    };
    let first = token(3600);
    let second = token(7200);
    assert_ne!(first, second);
    let state =
        AppStateBuilder::new(Arc::new(MockQueryExecutor::new()), edge::testing::edge()).build();
    let app = Router::new()
        .route("/probe", get(observe))
        .layer(Extension(Arc::new(Barrier::new(2))))
        .route_layer(axum::middleware::from_fn_with_state(
            state,
            defra_http::auth_middleware::auth_middleware,
        ));
    let request = |token: &str| {
        Request::builder()
            .uri("/probe")
            .header(HOST, "localhost:9181")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    let (a, b) = nuclei::join!(
        app.clone().oneshot(request(&first)),
        app.oneshot(request(&second))
    );
    for (response, expected) in [(a.unwrap(), first), (b.unwrap(), second)] {
        assert!(response.status().is_success());
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let observed: Vec<Option<String>> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(observed, vec![Some(expected.clone()), Some(expected)]);
    }
    assert_eq!(
        defra_core::signing::get_request_bearer_token(did.as_str()),
        None
    );
}

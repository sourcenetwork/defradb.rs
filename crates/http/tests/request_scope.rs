//! The auth middleware's request context reaches the handler and every
//! operations handle it binds, each request seeing its own caller.
use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{
        header::{AUTHORIZATION, HOST},
        Request,
    },
    routing::get,
    Json, Router,
};
use defra_core::RequestContext;
use defra_http::request_scope::{bound, CallerContext};
use defra_http::router::{AppState, AppStateBuilder, CollectionOperations};
use defra_http::MockQueryExecutor;
use identity::{new_token, Identity, RawIdentity};
use schema::CollectionVersion;
use tower::ServiceExt;

/// Answers with the identity acting in the request it is bound to.
struct BoundCollectionOperations(Option<String>);

#[async_trait]
impl CollectionOperations for BoundCollectionOperations {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn CollectionOperations>> {
        Some(Arc::new(Self(context.identity().map(str::to_owned))))
    }

    async fn add_collection(&self, _sdl: &str) -> Result<Vec<CollectionVersion>, String> {
        Err(self.0.clone().unwrap_or_else(|| "anonymous".to_string()))
    }
}

async fn observe(
    State(state): State<AppState>,
    CallerContext(context): CallerContext,
) -> Json<Vec<Option<String>>> {
    let schema = state.schema.clone().expect("schema operations");
    let bound_identity = bound(&schema, &context).add_collection("").await.err();
    Json(vec![context.identity().map(str::to_owned), bound_identity])
}

fn token(identity: &RawIdentity) -> String {
    String::from_utf8(
        new_token(
            identity,
            Duration::from_secs(3600),
            Some("localhost:9181".into()),
            None,
        )
        .unwrap(),
    )
    .unwrap()
}

#[nuclei::test]
async fn each_request_binds_its_own_caller() {
    let alice = RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
    let bob = RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
    let state = AppStateBuilder::new(Arc::new(MockQueryExecutor::new()), edge::testing::edge())
        .with_schema(Arc::new(BoundCollectionOperations(None)))
        .build();
    let app = Router::new()
        .route("/probe", get(observe))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            defra_http::auth_middleware::auth_middleware,
        ))
        .with_state(state);
    let request = |token: Option<String>| {
        let builder = Request::builder()
            .uri("/probe")
            .header(HOST, "localhost:9181");
        match token {
            Some(token) => builder.header(AUTHORIZATION, format!("Bearer {token}")),
            None => builder,
        }
        .body(Body::empty())
        .unwrap()
    };

    let (a, b, anonymous) = nuclei::join!(
        app.clone().oneshot(request(Some(token(&alice)))),
        app.clone().oneshot(request(Some(token(&bob)))),
        app.oneshot(request(None)),
    );
    for (response, caller) in [
        (a.unwrap(), Some(alice.did().unwrap().to_string())),
        (b.unwrap(), Some(bob.did().unwrap().to_string())),
        (anonymous.unwrap(), None),
    ] {
        assert!(response.status().is_success());
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let observed: Vec<Option<String>> = serde_json::from_slice(&bytes).unwrap();
        let bound_identity = caller.clone().or_else(|| Some("anonymous".to_string()));
        assert_eq!(observed, vec![caller, bound_identity]);
    }
}

async fn bearer(CallerContext(context): CallerContext) -> Json<Option<String>> {
    Json(
        context
            .identity()
            .and_then(|did| context.bearer_token_for(did)),
    )
}

#[nuclei::test]
async fn each_request_carries_its_own_bearer_in_its_context() {
    let identity = RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
    let first = token(&identity);
    let second = String::from_utf8(
        new_token(
            &identity,
            Duration::from_secs(7200),
            Some("localhost:9181".into()),
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_ne!(first, second);
    let state =
        AppStateBuilder::new(Arc::new(MockQueryExecutor::new()), edge::testing::edge()).build();
    let app = Router::new()
        .route("/bearer", get(bearer))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            defra_http::auth_middleware::auth_middleware,
        ))
        .with_state(state);
    let request = |token: &str| {
        Request::builder()
            .uri("/bearer")
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
        let observed: Option<String> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(observed, Some(expected));
    }
}

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use identity::{new_token, Identity, RawIdentity};
use serde::Deserialize;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::mock::{MockNodeAcpOperations, MockQueryExecutor};
use crate::router::NodePermission;
use crate::server::{RouterTransform, Server};

async fn filter(request: Request, next: Next) -> Response {
    if request
        .headers()
        .get(header::HOST)
        .is_some_and(|host| host == "blocked.example")
    {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    next.run(request).await
}

fn transform(constructions: Arc<AtomicUsize>) -> RouterTransform {
    Arc::new(move |router| {
        constructions.fetch_add(1, Ordering::SeqCst);
        router.layer(middleware::from_fn(filter))
    })
}

fn caller() -> (identity::Did, String, String) {
    let identity = RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
    let did = identity.did().unwrap();
    let bearer = |host: &str| {
        let token = new_token(&identity, Duration::from_secs(60), Some(host.into()), None).unwrap();
        format!("Bearer {}", String::from_utf8(token).unwrap())
    };
    (did, bearer("localhost:9181"), bearer("blocked.example"))
}

fn server(granted: bool, caller: &identity::Did) -> Server {
    let (owner, _, _) = self::caller();
    let mut nac = MockNodeAcpOperations::enabled_with_owner(owner);
    if granted {
        nac = nac.with_grant(caller.clone(), NodePermission::CollectionGet);
    }
    Server::new(MockQueryExecutor::new(), edge::testing::edge())
        .with_nac_arc(Arc::new(nac))
        .with_extra_routes(Router::new().route("/application", get(|| async { "application" })))
}

async fn status(router: Router, path: &str, allowed: bool, bearer: &str) -> StatusCode {
    let host = if allowed {
        "localhost:9181"
    } else {
        "blocked.example"
    };
    router
        .oneshot(
            Request::builder()
                .uri(path)
                .header(header::HOST, host)
                .header(header::AUTHORIZATION, bearer)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Case {
    input: Value,
    expected_status: u16,
}

fn model_cases(inputs: &[Value]) -> Vec<Case> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proofs/lean");
    let mut child = Command::new("lake")
        .current_dir(root)
        .args(["env", "lean", "--run", "HttpRouterComposition.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), inputs.len());
    for (case, input) in cases.iter().zip(inputs) {
        assert_eq!(&case.input, input);
    }
    cases
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn forwarding_filter_matches_executed_model() {
    let (caller, local_bearer, blocked_bearer) = caller();
    let (inputs, observed) = edge::testing::edge().block_on(async {
        let mut observed = Vec::new();
        let mut inputs = Vec::new();
        for granted in [false, true] {
            for path in [
                "/health-check",
                "/api/v0/schema",
                "/api/v1/schema/",
                "/application",
                "/missing/",
            ] {
                for allowed in [false, true] {
                    let bearer = if allowed {
                        &local_bearer
                    } else {
                        &blocked_bearer
                    };
                    let inner = status(
                        server(granted, &caller).router().unwrap(),
                        path,
                        allowed,
                        bearer,
                    )
                    .await;
                    if path.contains("schema") {
                        assert_eq!(
                            inner,
                            if granted {
                                StatusCode::OK
                            } else {
                                StatusCode::UNAUTHORIZED
                            }
                        );
                    }
                    for configured in [false, true] {
                        let counter = Arc::new(AtomicUsize::new(0));
                        let mut server = server(granted, &caller);
                        if configured {
                            server = server.with_router_transform(transform(counter.clone()));
                        }
                        let router = server.router().unwrap();
                        assert_eq!(counter.load(Ordering::SeqCst), usize::from(configured));
                        let actual = status(router, path, allowed, bearer).await;
                        assert_eq!(counter.load(Ordering::SeqCst), usize::from(configured));
                        inputs.push(json!({ "configured": configured, "allowed": allowed, "innerStatus": inner.as_u16() }));
                        observed.push((path, granted, configured, allowed, actual));
                    }
                }
            }
        }
        (inputs, observed)
    }).unwrap();
    for (case, observation) in model_cases(&inputs).into_iter().zip(observed) {
        assert_eq!(
            observation.4.as_u16(),
            case.expected_status,
            "{observation:?}"
        );
    }
}

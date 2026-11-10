use std::sync::Arc;

use crypto::PrivateKey;
use defra_core::signing::RemoteSigner;

use crate::client_signer::SessionSigner;

#[test]
fn the_connection_signer_checks_each_signature_before_answering() {
    edge::testing::edge()
        .block_on(async {
            let private = crypto::generate_ed25519().unwrap();
            let public = private.public_key();
            let (signer, mut requests) = SessionSigner::new(&hex::encode(public.raw())).unwrap();
            let payloads: Vec<Arc<[u8]>> =
                vec![Arc::from(b"one".as_slice()), Arc::from(b"two".as_slice())];
            let (answers, ()) = futures::join!(signer.sign_batch(payloads.clone(), None), async {
                let wave = requests.recv().await.unwrap();
                let mut signatures = wave
                    .payloads
                    .iter()
                    .map(|bytes| private.sign(bytes).unwrap())
                    .collect::<Vec<_>>();
                signatures[1][0] ^= 1;
                wave.answer.send(signatures).unwrap();
            });
            assert!(answers[0].is_ok());
            assert!(answers[1].is_err());
        })
        .unwrap();
}

#[test]
fn signing_endpoint_requires_an_authenticated_caller() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    edge::testing::edge()
        .block_on(async {
            let app = crate::router::create_router(
                Arc::new(crate::mock::MockQueryExecutor::new()),
                edge::testing::edge(),
            );
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/v0/graphql/client-sign")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        })
        .unwrap();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn admission_and_token_lifetime_match_the_model() {
    use nuclei::sync::Semaphore;
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::time::Duration;
    let inputs = serde_json::json!([
        {"used":0,"limit":1,"now":100,"expiration":120,"duration":90},
        {"used":1,"limit":1,"now":120,"expiration":120,"duration":90},
        {"used":1,"limit":1,"now":121,"expiration":120,"duration":90},
        {"used":0,"limit":1,"now":100,"expiration":1000,"duration":90}
    ]);
    let mut child = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "ClientSigningAdmission.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{inputs}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let cases: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let slots = Arc::new(Semaphore::new(1));
    let permit = crate::client_signing::try_session_slot(&slots).unwrap();
    assert_eq!(
        crate::client_signing::try_session_slot(&slots).is_ok(),
        cases[1]["admitted"].as_bool().unwrap()
    );
    drop(permit);
    assert_eq!(
        crate::client_signing::try_session_slot(&slots).is_ok(),
        cases[0]["admitted"].as_bool().unwrap()
    );
    for case in cases.as_array().unwrap() {
        let input = &case["input"];
        let lifetime = crate::client_signing::session_lifetime(
            Duration::from_secs(input["now"].as_u64().unwrap()),
            input["expiration"].as_u64().unwrap(),
        );
        assert_eq!(
            lifetime.map_or(0, |duration| duration.as_secs()),
            case["lifetime"].as_u64().unwrap()
        );
        assert_eq!(lifetime.is_some(), case["unexpired"].as_bool().unwrap());
        let context = defra_core::RequestContext::default()
            .with_authorization_expiration(input["expiration"].as_u64());
        assert_eq!(
            context.authorization_valid_at(input["now"].as_u64().unwrap()),
            case["unexpired"].as_bool().unwrap()
        );
    }
}

#[test]
fn expired_bearer_is_refused_even_with_the_default_clock_skew() {
    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
    };
    use tower::ServiceExt;
    edge::testing::edge()
        .block_on(async {
            let identity =
                identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap())
                    .unwrap();
            let token = identity::new_token(
                &identity,
                std::time::Duration::ZERO,
                Some("localhost".into()),
                None,
            )
            .unwrap();
            let app = crate::router::create_router(
                Arc::new(crate::mock::MockQueryExecutor::new()),
                edge::testing::edge(),
            );
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/v0/graphql/client-sign")
                        .header(header::HOST, "localhost")
                        .header(
                            header::AUTHORIZATION,
                            format!("Bearer {}", String::from_utf8(token).unwrap()),
                        )
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        })
        .unwrap();
}

#[test]
fn disconnected_and_expired_connections_release_their_slots() {
    use async_tungstenite::tungstenite::client::IntoClientRequest;
    use futures::StreamExt;
    use std::time::Duration;
    edge::testing::edge()
        .block_on(async {
            let edge = edge::testing::edge();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let state = crate::router::AppStateBuilder::new(
                Arc::new(crate::mock::MockQueryExecutor::new()),
                edge.clone(),
            )
            .build();
            let slots = state.signing_slots.clone();
            let app = crate::router::create_router_with_state(state);
            let server = edge.spawn(crate::server::serve_router(listener, app, edge.clone()));
            let identity =
                identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap())
                    .unwrap();
            for expires in [false, true] {
                let duration = if expires {
                    Duration::from_secs(3)
                } else {
                    Duration::from_secs(30)
                };
                let token =
                    identity::new_token(&identity, duration, Some(address.to_string()), None)
                        .unwrap();
                let mut request = format!("ws://{address}/api/v0/graphql/client-sign")
                    .into_client_request()
                    .unwrap();
                request.headers_mut().insert(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {}", String::from_utf8(token).unwrap())
                        .parse()
                        .unwrap(),
                );
                let stream = net::connect(request.uri()).await.unwrap();
                let (mut socket, _) = async_tungstenite::client_async(request, stream)
                    .await
                    .unwrap();
                assert!(slots.clone().try_acquire_many_owned(32).is_err());
                if expires {
                    let _ = nuclei::time::timeout(Duration::from_secs(5), socket.next())
                        .await
                        .expect("expired socket closes");
                } else {
                    socket.close(None).await.unwrap();
                }
                let permit = nuclei::time::timeout(
                    Duration::from_secs(5),
                    slots.clone().acquire_many_owned(32),
                )
                .await
                .expect("session releases its slot")
                .unwrap();
                drop(permit);
            }
            server.abort();
        })
        .unwrap();
}

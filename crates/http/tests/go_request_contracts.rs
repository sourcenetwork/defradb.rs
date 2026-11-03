//! Go's request contracts for the routes #1411 adds.
//!
//! Resolving the path is only half of it: Go's client sends a raw policy body,
//! marshals the view transform as `TransformCID`, and selects a refresh with
//! query parameters and no body at all.

mod common;
use common::*;

// ---------------------------------------------------------------------------
// Policy request contract
// ---------------------------------------------------------------------------

/// Go sends the policy as a raw body with no content type. An extractor that
/// demanded JSON would reject every real Go request.
#[test]
fn a_raw_policy_body_needs_no_content_type() {
    edge::testing::edge()
        .block_on(async {
            let (status, body) = Call::post("/api/v0/acp/document/policy")
                .body(POLICY)
                .authenticated()
                .send()
                .await;
            assert_eq!(status, StatusCode::OK, "{body}");
        })
        .unwrap();
}

#[test]
fn the_policy_response_is_gos_shape() {
    edge::testing::edge()
        .block_on(async {
            let (status, body) = Call::post("/api/v0/acp/document/policy")
                .body(POLICY)
                .authenticated()
                .send()
                .await;
            assert_eq!(status, StatusCode::OK);
            let parsed: serde_json::Value = serde_json::from_str(&body).expect("JSON response");
            assert!(parsed.get("PolicyID").is_some(), "{body}");
        })
        .unwrap();
}

// ---------------------------------------------------------------------------
// AddView request contract
// ---------------------------------------------------------------------------

async fn transform_reaching_the_view_layer(body: &str) -> (StatusCode, Option<Option<String>>) {
    let view = Arc::new(RecordingViewOps::default());
    let (status, _) = Call::post("/api/v0/view")
        .json(body)
        .send_to(router_with(Arc::clone(&view)))
        .await;
    let seen = view.add_view_transform.load().map(|t| (*t).clone());
    (status, seen)
}

/// Go's client marshals the field as `TransformCID`. Read as `Transform`, the
/// key was silently ignored and the view was built without its lens transform.
#[test]
fn gos_transform_cid_survives() {
    edge::testing::edge()
        .block_on(async {
            let (status, seen) = transform_reaching_the_view_layer(
                r#"{"Query":"q","SDL":"s","TransformCID":"bafy"}"#,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(seen, Some(Some("bafy".to_string())));
        })
        .unwrap();
}

/// Go marshals an absent option as `null`, not by omitting the key.
#[test]
fn gos_null_transform_cid_is_none() {
    edge::testing::edge()
        .block_on(async {
            let (status, seen) =
                transform_reaching_the_view_layer(r#"{"Query":"q","SDL":"s","TransformCID":null}"#)
                    .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(seen, Some(None));
        })
        .unwrap();
}

#[test]
fn the_rust_transform_key_still_works() {
    edge::testing::edge()
        .block_on(async {
            let (status, seen) =
                transform_reaching_the_view_layer(r#"{"Query":"q","SDL":"s","Transform":"bafy"}"#)
                    .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(seen, Some(Some("bafy".to_string())));
        })
        .unwrap();
}

/// Both spellings at once is ambiguous, so it is a client error rather than a
/// silent winner.
#[test]
fn both_transform_keys_is_a_client_error() {
    edge::testing::edge()
        .block_on(async {
            let (status, _) = transform_reaching_the_view_layer(
                r#"{"Query":"q","SDL":"s","Transform":"a","TransformCID":"b"}"#,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        })
        .unwrap();
}

#[test]
fn a_view_without_a_query_or_sdl_is_rejected() {
    edge::testing::edge()
        .block_on(async {
            for body in [r#"{"SDL":"s"}"#, r#"{"Query":"q"}"#] {
                let (status, _) = transform_reaching_the_view_layer(body).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            }
        })
        .unwrap();
}

// ---------------------------------------------------------------------------
// RefreshViews request contract
// ---------------------------------------------------------------------------

async fn refresh_options_for(call: Call) -> (StatusCode, Option<db::CollectionSelector>) {
    let view = Arc::new(RecordingViewOps::default());
    let (status, _) = call.send_to(router_with(Arc::clone(&view))).await;
    let seen = view.refresh.load().map(|r| (*r).clone());
    (status, seen)
}

/// Go's real refresh request: `setDefaultHeaders` sets
/// `Content-Type: application/json` on every request (`http/http_client.go:94`)
/// and `RefreshViews` passes a nil body, so the header is present and the body
/// is empty. Against `Json<T>` that is a 400 "EOF while parsing", which is why
/// the body is read directly instead.
#[test]
fn gos_real_refresh_request_is_accepted() {
    edge::testing::edge()
        .block_on(async {
            let (status, options) =
                refresh_options_for(Call::post("/api/v0/view/refresh").json("")).await;
            assert_eq!(status, StatusCode::OK);
            let options = options.expect("the view layer must be reached");
            assert_eq!(options.names, None);
            assert!(!options.get_inactive);
        })
        .unwrap();
}

/// A client that omits the header entirely, which `Json` answers with 415.
#[test]
fn a_refresh_without_any_content_type_is_accepted() {
    edge::testing::edge()
        .block_on(async {
            let (status, options) = refresh_options_for(Call::post("/api/v0/view/refresh")).await;
            assert_eq!(status, StatusCode::OK);
            let options = options.expect("the view layer must be reached");
            assert_eq!(options.names, None);
            assert!(!options.get_inactive);
        })
        .unwrap();
}

#[test]
fn gos_name_selector_narrows_the_refresh() {
    edge::testing::edge()
        .block_on(async {
            let (status, options) =
                refresh_options_for(Call::post("/api/v0/view/refresh?name=Foo")).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(options.unwrap().names, Some(vec!["Foo".to_string()]));
        })
        .unwrap();
}

#[test]
fn gos_remaining_selectors_are_honoured() {
    edge::testing::edge()
        .block_on(async {
            let (status, options) = refresh_options_for(Call::post(
                "/api/v0/view/refresh?version_id=v1&collection_id=c1&get_inactive=true",
            ))
            .await;
            assert_eq!(status, StatusCode::OK);
            let options = options.unwrap();
            assert_eq!(options.version_id.as_deref(), Some("v1"));
            assert_eq!(options.collection_id.as_deref(), Some("c1"));
            assert!(options.get_inactive);
        })
        .unwrap();
}

/// Go returns 400 from `strconv.ParseBool` for the same input.
#[test]
fn a_malformed_bool_is_rejected() {
    edge::testing::edge()
        .block_on(async {
            let (status, options) =
                refresh_options_for(Call::post("/api/v0/view/refresh?get_inactive=notabool")).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(options.is_none(), "a rejected request must not refresh");
        })
        .unwrap();
}

#[test]
fn the_rust_names_body_still_works() {
    edge::testing::edge()
        .block_on(async {
            let (status, options) = refresh_options_for(
                Call::post("/api/v0/view/refresh").json(r#"{"Names":["A","B"]}"#),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                options.unwrap().names,
                Some(vec!["A".to_string(), "B".to_string()])
            );
        })
        .unwrap();
}

/// Both are "restrict to these views", so they union. Neither silently widens
/// the refresh to everything.
#[test]
fn body_names_and_query_name_union() {
    edge::testing::edge()
        .block_on(async {
            let (status, options) = refresh_options_for(
                Call::post("/api/v0/view/refresh?name=Foo").json(r#"{"Names":["A"]}"#),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                options.unwrap().names,
                Some(vec!["A".to_string(), "Foo".to_string()])
            );
        })
        .unwrap();
}

// ---------------------------------------------------------------------------
// The rest of Go's ACP surface
// ---------------------------------------------------------------------------

/// Every path and method Go's ACP client calls, read from `http/client_acp.go`.
/// The issue asserted that only `document/policy` diverged; this holds the
/// claim to it, so the next divergence is a test failure rather than a report.
const GO_ACP_CLIENT_CALLS: &[(&str, &str)] = &[
    ("POST", "/api/v1/acp/document/policy"),
    ("POST", "/api/v1/acp/document/relationship"),
    ("DELETE", "/api/v1/acp/document/relationship"),
    ("POST", "/api/v1/acp/node/relationship"),
    ("DELETE", "/api/v1/acp/node/relationship"),
    ("POST", "/api/v1/acp/node/re-enable"),
    ("POST", "/api/v1/acp/node/disable"),
    ("GET", "/api/v1/acp/node/status"),
];

#[test]
fn every_acp_path_gos_client_calls_resolves() {
    edge::testing::edge()
        .block_on(async {
            for (method, path) in GO_ACP_CLIENT_CALLS {
                let method = Method::from_bytes(method.as_bytes()).expect("a valid method");
                let (status, body) = Call::post(path).method(method.clone()).send().await;
                assert_ne!(
                    status,
                    StatusCode::NOT_FOUND,
                    "{method} {path} is not registered: {body}"
                );
                assert_ne!(
                    status,
                    StatusCode::METHOD_NOT_ALLOWED,
                    "{method} {path} is registered under a different method: {body}"
                );
            }
        })
        .unwrap();
}

// ---------------------------------------------------------------------------
// Malformed-body status
// ---------------------------------------------------------------------------

/// Go answers every body-parse failure with 400 (`http/handler_store.go:236`).
/// axum's own `Json` rejection is 422 for a body that is valid JSON but does
/// not fit the type, which is a status a Go-compatible client does not expect.
/// This is the same divergence the issue's comment recorded for
/// `acp/document/relationship`, and it must not ship on the routes added here.
#[test]
fn a_malformed_view_body_is_gos_400_not_422() {
    edge::testing::edge()
        .block_on(async {
            for body in [
                r#"{"SDL":"s"}"#,
                r#"{"Query":"q"}"#,
                r#"{"Query":5,"SDL":"s"}"#,
                r#"{"Query":"q","SDL":"s","Transform":"a","TransformCID":"b"}"#,
            ] {
                for path in ["/api/v0/view"] {
                    let (status, _) = Call::post(path).json(body).send().await;
                    assert_eq!(status, StatusCode::BAD_REQUEST, "{path} with {body}");
                }
            }
        })
        .unwrap();
}

/// Not valid JSON at all is 400 on both runtimes, and stays so.
#[test]
fn an_unparseable_view_body_is_400() {
    edge::testing::edge()
        .block_on(async {
            let (status, _) = Call::post("/api/v0/view").json("{not json").send().await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        })
        .unwrap();
}

/// `Json` silently discards a body sent without `Content-Type: application/json`.
/// For this route that would widen a one-view refresh to every view, with a 200
/// and no way for the caller to tell, so the body is read directly instead.
#[test]
fn a_body_without_a_content_type_is_still_honoured() {
    edge::testing::edge()
        .block_on(async {
            let mut call = Call::post("/api/v0/view/refresh");
            call.body = r#"{"Names":["A"]}"#.to_string();

            let (status, options) = refresh_options_for(call).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(options.unwrap().names, Some(vec!["A".to_string()]));
        })
        .unwrap();
}

/// A body that is present but unparseable is refused, not ignored, and with
/// Go's 400 rather than axum's 422.
#[test]
fn an_unparseable_refresh_body_is_gos_400() {
    edge::testing::edge()
        .block_on(async {
            for body in [r#"{"Names":5}"#, "{not json"] {
                let mut call = Call::post("/api/v0/view/refresh");
                call.body = body.to_string();
                let (status, options) = refresh_options_for(call).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
                assert!(
                    options.is_none(),
                    "a refused request must not refresh: {body}"
                );
            }
        })
        .unwrap();
}

/// An empty body selects nothing, which is what Go does with the body it never
/// reads, so every view refreshes.
#[test]
fn an_empty_body_selects_nothing() {
    edge::testing::edge()
        .block_on(async {
            for call in [
                Call::post("/api/v0/view/refresh"),
                Call::post("/api/v0/view/refresh").json(""),
                Call::post("/api/v0/view/refresh").json("  "),
            ] {
                let (status, options) = refresh_options_for(call).await;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(options.expect("the view layer must be reached").names, None);
            }
        })
        .unwrap();
}

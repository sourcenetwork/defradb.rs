//! The native node under test, driven through its HTTP API with fetch.

use std::time::Duration;

use serde_json::{json, Value};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

pub const API: &str = env!("DEFRA_E2E_API");
pub const RELAY: &str = env!("DEFRA_E2E_RELAY");

const REPLICATION_TIMEOUT: Duration = Duration::from_secs(60);

/// A peer address that reaches `endpoint_id` through the node's relay.
pub fn address(endpoint_id: &str) -> String {
    format!("{endpoint_id}@{RELAY}")
}

/// The bare endpoint id among the node's advertised addresses, which is what a
/// relay address is built from.
pub async fn endpoint_id() -> String {
    http("GET", "/api/v0/p2p/info", "application/json", "", None)
        .await
        .as_array()
        .expect("p2p info is an address list")
        .iter()
        .filter_map(Value::as_str)
        .find(|address| address.len() == 64 && address.chars().all(|c| c.is_ascii_hexdigit()))
        .expect("the node advertises its endpoint id")
        .to_string()
}

pub async fn add_schema(sdl: &str, token: Option<&str>) {
    http("POST", "/api/v0/schema", "text/plain", sdl, token).await;
}

/// A node with document ACP only takes pushes for collections it follows.
pub async fn subscribe(collections: &[&str]) {
    post_json("/api/v0/p2p/collections", json!(collections), None).await;
}

pub async fn replicate_to(address: &str, collections: &[&str]) {
    post_json(
        "/api/v0/p2p/replicators",
        json!({ "Collections": collections, "Addresses": [address] }),
        None,
    )
    .await;
}

/// The `data` of a query that must succeed.
pub async fn graphql(query: &str, token: Option<&str>) -> Value {
    let response = post_json("/api/v0/graphql", json!({ "query": query }), token).await;
    assert!(
        response["errors"]
            .as_array()
            .is_none_or(|errors| errors.is_empty()),
        "graphql failed: {response}"
    );
    response["data"].clone()
}

pub async fn post_json(path: &str, body: Value, token: Option<&str>) -> Value {
    http("POST", path, "application/json", &body.to_string(), token).await
}

pub async fn http(
    method: &str,
    path: &str,
    content_type: &str,
    body: &str,
    token: Option<&str>,
) -> Value {
    let headers = Headers::new().unwrap();
    headers.set("Content-Type", content_type).unwrap();
    if let Some(token) = token {
        headers
            .set("Authorization", &format!("Bearer {token}"))
            .unwrap();
    }
    let init = RequestInit::new();
    init.set_method(method);
    init.set_headers(&headers);
    if !body.is_empty() {
        init.set_body(&JsValue::from_str(body));
    }
    let request = Request::new_with_str_and_init(&format!("{API}{path}"), &init).unwrap();
    let response: Response =
        JsFuture::from(web_sys::window().unwrap().fetch_with_request(&request))
            .await
            .unwrap()
            .dyn_into()
            .unwrap();
    let text = JsFuture::from(response.text().unwrap())
        .await
        .unwrap()
        .as_string()
        .unwrap_or_default();
    assert!(
        response.ok(),
        "{method} {path} returned {}: {text}",
        response.status()
    );
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

pub async fn wait_for<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let started = js_sys::Date::now();
    loop {
        if check().await {
            return;
        }
        let elapsed = Duration::from_millis((js_sys::Date::now() - started) as u64);
        assert!(
            elapsed < REPLICATION_TIMEOUT,
            "timed out waiting for {what}"
        );
        gloo_timers::future::TimeoutFuture::new(250).await;
    }
}

/// The `_docID` a single create mutation returned. Read by position because
/// `create_X` answers under Go's `add_X` name.
pub fn created_doc_id(data: &Value) -> String {
    let created = data
        .as_object()
        .and_then(|fields| fields.values().next())
        .unwrap_or(data);
    let created = created.as_array().map_or(created, |list| &list[0]);
    created["_docID"]
        .as_str()
        .unwrap_or_else(|| panic!("no _docID in {created}"))
        .to_string()
}

pub fn doc_ids(rows: &Value) -> Vec<String> {
    rows.as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["_docID"].as_str())
        .map(String::from)
        .collect()
}

//! `GET /openapi.json` describes the routes this node serves, not Go's.

use integration_test::TestCluster;
use net::http::header::CONTENT_TYPE;
use net::http::{Client, Method, StatusCode};
use serde_json::Value;

/// Only a Rust node answers this document: a Go node answers with Go's own, and
/// the CLI reaches a Rust node's HTTP API only for the commands it carries. The
/// harness's other clients are skipped rather than listed as known failures.
fn is_rust_node() -> bool {
    !matches!(
        std::env::var("DEFRA_CLIENT").as_deref(),
        Ok("go-http") | Ok("cli")
    )
}

#[nuclei::test]
#[mantra_macros::req_test("defra.http.openapi")]
async fn the_document_describes_the_routes_this_node_serves() {
    if !is_rust_node() {
        return;
    }
    edge::compat(async {
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        let base = cluster.api_url(0);
        let client = Client::new(edge::testing::edge());

        let (status, content_type, document) = fetch(&client, base, "/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type, "application/json");
        assert_eq!(document["openapi"], "3.0.3");

        // A route Go has no equivalent for: only this server's own document
        // lists it, which is the point of the endpoint.
        assert!(
            document["paths"]["/p2p/manage"]["post"].is_object(),
            "a Rust-only route is missing from the document"
        );

        // A route Go documents, carried with Go's own request body.
        assert!(
            document["paths"]["/collections/{name}"]["post"]["requestBody"].is_object(),
            "Go's operation for the collection document route lost its request body"
        );

        // Root-mounted, so it cannot inherit the document's `/api/v0` mounts.
        assert_eq!(
            document["paths"]["/health-check"]["servers"][0]["url"], "/",
            "a root-mounted route inherited the versioned mounts"
        );

        // Go's document describes 64 operations; this one has to describe more.
        let operations = document["paths"]
            .as_object()
            .expect("paths")
            .values()
            .map(|item| {
                item.as_object()
                    .expect("path item")
                    .keys()
                    .filter(|key| {
                        matches!(
                            key.as_str(),
                            "get"
                                | "put"
                                | "post"
                                | "delete"
                                | "options"
                                | "head"
                                | "patch"
                                | "trace"
                        )
                    })
                    .count()
            })
            .sum::<usize>();
        assert!(
            operations > 64,
            "the document describes {operations} operations, no more than Go's 64"
        );

        let (alias_status, alias_type, alias) = fetch(&client, base, "/openapi.rs.json").await;
        assert_eq!(alias_status, StatusCode::OK);
        assert_eq!(alias_type, "application/json");
        assert_eq!(alias, document, "the alias answers a different document");
    })
    .await;
}

async fn fetch(client: &Client, base: &str, path: &str) -> (StatusCode, String, Value) {
    let response = client
        .request(Method::GET, format!("{base}{path}"))
        .send()
        .await
        .expect("send the document request");
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let document = response.json().await.expect("decode the document");
    (status, content_type, document)
}

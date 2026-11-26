//! A running node answers `GET /openapi.json` with the document of the routes
//! it serves, and `GET /openapi.rs.json` with the same document. Only a Rust
//! node serves it, so the scenario does not run under go-http.

use integration_test::actions::OpenApiDocument;
use integration_test::m;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

/// A root route with no security, which the document must not mount under
/// `/api/v0`.
fn exempt_root_route() -> Value {
    m::object(json!({
        "servers": [{ "url": "/" }],
        "get": m::object(json!({ "security": [], "x-defra-permission": "exempt" })),
    }))
}

/// What the document must say: Go's dialect, Go's operations unchanged, and
/// the security each generated operation carries.
fn served_document() -> Value {
    json!({
        "status": 200,
        "contentType": "application/json",
        "document": m::all(json!([
            m::object(json!({
                "openapi": "3.0.3",
                "paths": m::object(json!({
                    "/graphql": m::object(json!({
                        "post": m::object(json!({
                            "operationId": "post_graphql",
                            "tags": ["graphql"],
                        })),
                    })),
                    "/health-check": exempt_root_route(),
                    "/openapi.json": exempt_root_route(),
                    "/openapi.rs.json": exempt_root_route(),
                    "/version": m::object(json!({
                        "get": m::object(json!({ "security": [] })),
                    })),
                    "/events": m::object(json!({
                        "get": m::object(json!({
                            "security": [{}, { "bearerToken": [] }],
                            "x-defra-permission": "dynamic",
                        })),
                    })),
                })),
            })),
            m::same("document"),
        ])),
    })
}

#[test]
#[mantra_macros::req_test("defra.http.openapi")]
fn a_node_serves_the_document_of_its_routes_and_its_alias() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            OpenApiDocument {
                expected_data: served_document(),
                ..Default::default()
            }
            .into(),
            OpenApiDocument {
                alias: true,
                expected_data: served_document(),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

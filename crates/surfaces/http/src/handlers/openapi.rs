use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;

const OPENAPI_JSON: &str = include_str!("openapi-go-e152b5cab.json");

/// GET /openapi.json
///
/// Go's pinned document, byte for byte: a client generated from Go's spec keeps
/// working, and `document_tracks_the_pinned_go_baseline` keeps it current.
pub async fn get() -> impl IntoResponse {
    ([(CONTENT_TYPE, "application/json")], OPENAPI_JSON)
}

/// GET /openapi.rs.json
///
/// The same API as this server actually serves it: Go's operations carried
/// verbatim, every route in `route_permissions::ROUTES` added (#3263).
pub async fn generated() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "application/json")],
        crate::openapi::document().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Method, Request, StatusCode};
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::mock::MockQueryExecutor;
    use crate::Server;

    fn concrete_path(path: &str) -> String {
        ["sender", "data", "name", "docID", "field", "index", "id"]
            .into_iter()
            .fold(path.to_string(), |path, parameter| {
                path.replace(&format!("{{{parameter}}}"), "probe")
            })
    }

    #[nuclei::test]
    async fn serves_the_pinned_go_openapi_document() {
        let response = Server::new(MockQueryExecutor::new(), edge::testing::edge())
            .router()
            .unwrap()
            .oneshot(
                Request::builder()
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_TYPE], "application/json");
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let document: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(document["openapi"], "3.0.3");
        assert_eq!(document["paths"].as_object().unwrap().len(), 44);
        assert_eq!(
            document["paths"]
                .as_object()
                .unwrap()
                .values()
                .map(|path| path.as_object().unwrap().len())
                .sum::<usize>(),
            64
        );
        assert_eq!(body.as_ref(), OPENAPI_JSON.as_bytes());
    }

    /// The generated document is served, lists a route Go does not document,
    /// and keeps Go's schemas for the routes Go does.
    #[nuclei::test]
    async fn serves_the_document_generated_from_the_route_table() {
        let response = Server::new(MockQueryExecutor::new(), edge::testing::edge())
            .router()
            .unwrap()
            .oneshot(
                Request::builder()
                    .uri("/openapi.rs.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_TYPE], "application/json");
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let document: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(document["openapi"], crate::openapi::OPENAPI_VERSION);
        // Rust-only, so only this endpoint documents it.
        assert!(document["paths"]["/p2p/manage"]["post"].is_object());
        // Root-mounted, so it cannot inherit the document's `/api/v0` servers.
        assert_eq!(document["paths"]["/health-check"]["servers"][0]["url"], "/");
        // Go-documented, so Go's request body rides along.
        assert!(document["paths"]["/collections/{name}"]["post"]["requestBody"].is_object());
    }

    #[nuclei::test]
    async fn every_documented_path_exists_in_the_rust_router() {
        let app = Server::new(MockQueryExecutor::new(), edge::testing::edge())
            .router()
            .unwrap();
        let document: Value = serde_json::from_str(OPENAPI_JSON).unwrap();

        for path in document["paths"].as_object().unwrap().keys() {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::TRACE)
                        .uri(format!("/api/v0{}", concrete_path(path)))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "documented path does not resolve: {path}"
            );
        }
    }

    #[test]
    fn document_tracks_the_pinned_go_baseline() {
        assert_eq!(
            defra_version::GO_COMPAT_COMMIT,
            "e152b5cab",
            "refresh the OpenAPI snapshot when the Go compatibility baseline changes"
        );
    }
}

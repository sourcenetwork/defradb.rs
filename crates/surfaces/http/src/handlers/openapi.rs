use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;

/// GET /openapi.json
///
/// The API as this server serves it: Go's document with every route this node
/// adds, generated from the route table (#3263). It used to be Go's file
/// verbatim; `go_rust_http_contract` in the integration tests and
/// [`crate::openapi`]'s tests hold Go's operations to coming out identical.
pub async fn get() -> impl IntoResponse {
    served()
}

/// GET /openapi.rs.json
///
/// The same document, under the name it was introduced with. An alias for one
/// release, so a client that switched to it when it was the only truthful one
/// is not broken by the move.
pub async fn generated() -> impl IntoResponse {
    served()
}

fn served() -> impl IntoResponse {
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

    /// Go's document at the compatibility baseline, as the independent oracle
    /// for these tests: the served body is generated from the route table, so
    /// comparing it with itself would prove nothing about Go. `crate::openapi`
    /// embeds the same file as its merge base.
    const OPENAPI_GO: &str = include_str!("openapi-go-307364d9d.json");

    fn concrete_path(path: &str) -> String {
        ["sender", "data", "name", "docID", "field", "index", "id"]
            .into_iter()
            .fold(path.to_string(), |path, parameter| {
                path.replace(&format!("{{{parameter}}}"), "probe")
            })
    }

    async fn fetch(uri: &str) -> (StatusCode, String, Value) {
        let response = Server::new(MockQueryExecutor::new(), edge::testing::edge())
            .router()
            .unwrap()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let content_type = response.headers()[CONTENT_TYPE]
            .to_str()
            .unwrap_or_default()
            .to_string();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let document = serde_json::from_slice(&body).expect("the document parses");
        (status, content_type, document)
    }

    /// The served document describes this server, not only Go: it carries
    /// routes Go has no equivalent for, which is why `/openapi.json` is no
    /// longer Go's file byte for byte.
    #[nuclei::test]
    async fn serves_a_document_that_describes_this_server() {
        let (status, content_type, document) = fetch("/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type, "application/json");
        assert_eq!(document["openapi"], crate::openapi::OPENAPI_VERSION);
        assert!(document["paths"]["/p2p/manage"]["post"].is_object());
        assert!(document["paths"]["/batch/start"]["post"].is_object());
    }

    /// The alias introduced with the generated document serves the same thing,
    /// so a client that adopted it keeps working.
    #[nuclei::test]
    async fn the_short_lived_alias_serves_the_same_document() {
        let (_, _, from_gos_name) = fetch("/openapi.json").await;
        let (_, _, from_alias) = fetch("/openapi.rs.json").await;
        assert_eq!(from_gos_name, from_alias);
    }

    /// Go's operations are carried verbatim, so a client generated from this
    /// server's document is no worse off than one generated from Go's.
    #[nuclei::test]
    async fn gos_operations_survive_the_replacement() {
        let (_, _, document) = fetch("/openapi.json").await;
        let go: Value = serde_json::from_str(OPENAPI_GO).expect("the baseline parses");
        let mut compared = 0;
        for (path, item) in go["paths"].as_object().expect("paths") {
            for (method, operation) in item.as_object().expect("path item") {
                if !is_operation(method) {
                    continue;
                }
                assert_eq!(
                    &document["paths"][path][method], operation,
                    "Go's {method} {path} did not survive"
                );
                compared += 1;
            }
        }
        assert_eq!(compared, 64, "Go's document has 64 operations");
    }

    /// Every path Go documents still resolves in this router. The baseline is
    /// read from the tree rather than from the served body: the served body is
    /// generated from the route table, so comparing it with itself would prove
    /// nothing about Go.
    #[nuclei::test]
    async fn every_go_documented_path_exists_in_the_rust_router() {
        let app = Server::new(MockQueryExecutor::new(), edge::testing::edge())
            .router()
            .unwrap();
        let go: Value = serde_json::from_str(OPENAPI_GO).expect("the baseline parses");

        for path in go["paths"].as_object().expect("paths").keys() {
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
            "307364d9d",
            "refresh the OpenAPI snapshot when the Go compatibility baseline changes"
        );
    }

    fn is_operation(key: &str) -> bool {
        matches!(
            key,
            "get" | "put" | "post" | "delete" | "options" | "head" | "patch" | "trace"
        )
    }
}

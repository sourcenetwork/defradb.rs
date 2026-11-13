//! The routes that check `get-collection` first are exactly Go's
//! `CollectionMiddleware` group (`http/handler.go`, `handler_collection.go`).

use axum::http::Method;
use defra_http::route_permissions::reads_collection_first;

#[test]
fn collection_handler_routes_read_their_collection_first() {
    let first = [
        ("/api/v0/collections/{name}", Method::POST),
        ("/api/v0/collections/{name}", Method::PATCH),
        ("/api/v0/collections/{name}", Method::DELETE),
        ("/api/v0/collections/{name}/indexes", Method::POST),
        ("/api/v0/collections/{name}/indexes", Method::GET),
        ("/api/v0/collections/{name}/indexes/{index}", Method::DELETE),
        ("/api/v0/collections/{name}/encrypted-indexes", Method::POST),
        ("/api/v0/collections/{name}/encrypted-indexes", Method::GET),
        (
            "/api/v0/collections/{name}/encrypted-indexes/{field}",
            Method::DELETE,
        ),
        ("/api/v0/collections/{name}/truncate", Method::DELETE),
        ("/api/v0/collections/{name}/document/{docID}", Method::GET),
        ("/api/v0/collections/{name}/document/{docID}", Method::PATCH),
        (
            "/api/v1/collections/{name}/document/{docID}",
            Method::DELETE,
        ),
        ("/api/collections/{name}/truncate", Method::DELETE),
    ];
    for (path, method) in &first {
        assert!(reads_collection_first(path, method), "{method} {path}");
    }
}

#[test]
fn other_routes_do_not() {
    let not_first = [
        ("/api/v0/collections/{name}", Method::GET),
        ("/api/v0/collections/{name}/describe", Method::GET),
        ("/api/v0/collections/{name}/exists", Method::GET),
        ("/api/v0/collections/indexes", Method::GET),
        ("/api/v0/collections", Method::POST),
        ("/api/purge", Method::POST),
        ("/api/v0/graphql", Method::POST),
    ];
    for (path, method) in &not_first {
        assert!(!reads_collection_first(path, method), "{method} {path}");
    }
}

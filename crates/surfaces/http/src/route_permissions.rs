//! The routes the server serves, and the permission each one needs.
//!
//! [`ROUTES`] is the one list: the auth middleware reads a route's permission
//! from it, the OpenAPI document is generated from it, and
//! `tests/route_permissions.rs` proves every row resolves in the router.
//! Axum exposes no route enumeration, so a route registered without a row here
//! is caught by the source scan in this module's tests instead.

use std::borrow::Cow;

use axum::http::Method;

use crate::router::NodePermission;

pub use crate::route_table::{Route, ROUTES};

/// Permission requirement for a route.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RoutePermission {
    /// Route requires this specific permission.
    Required(NodePermission),
    /// Route has custom auth logic in the handler (e.g., NAC enable/disable, batch start/sign).
    Dynamic,
    /// Route is intentionally public (health, version, batch verify).
    Exempt,
    /// Route needs identity extracted but no specific permission check
    /// (e.g., tx handlers where permissions are per-operation).
    IdentityOnly,
}

/// The row for a `MatchedPath` template and method, if the route is listed.
///
/// A linear scan: the table is a hundred short strings and most rows differ on
/// length alone. A hash lookup is the next step if a profile ever asks for it.
pub fn route(path: &str, method: &Method) -> Option<&'static Route> {
    let path = normalize_api_version(path);
    ROUTES
        .iter()
        .find(|candidate| candidate.path == path && candidate.matches(method))
}

/// Whether the route reads its collection before it runs, as Go's
/// `CollectionMiddleware` does.
pub fn reads_collection_first(path: &str, method: &Method) -> bool {
    route(path, method).is_some_and(|found| found.collection_first)
}

/// Look up the permission requirement for a route.
///
/// The `path` parameter is the Axum `MatchedPath` template string
/// (e.g., `/api/v0/collections/{name}/document/{docID}`). The `method` is the
/// HTTP method.
///
/// Unknown routes return `Required(DocumentRead)` as a safe default: when NAC
/// is enabled, this blocks unauthenticated access.
pub fn route_permission(path: &str, method: &Method) -> RoutePermission {
    match route(path, method) {
        Some(found) => found.permission.clone(),
        None => {
            tracing::warn!(
                path = path,
                method = %method,
                "Route not in permission table — applying safe default (DocumentRead)"
            );
            RoutePermission::Required(NodePermission::DocumentRead)
        }
    }
}

/// Fold every mount of the API router onto its `/api/v0` key.
///
/// `go_paths::API_PREFIXES` is what `routes.rs` mounts the one route set at,
/// so it is also what has to be folded here. Deriving from the same constant
/// is the point: a prefix added there without a matching entry here would look
/// up the whole route set unfolded, and every route under it would fall to the
/// safe default and be enforced as `DocumentRead`.
///
/// The longest match wins, so the list stays order-independent: `/api/v0/x`
/// must fold on `/api/v0` and not on the bare `/api`.
///
/// Note this rewrites rather than rejects: `/api/v2/collections` becomes
/// `/api/v0/v2/collections`, which is safe only because no row's path begins
/// with `/api/v0/v`, and which lands on the safe default as it should.
fn normalize_api_version(path: &str) -> Cow<'_, str> {
    let matched = crate::go_paths::API_PREFIXES
        .iter()
        .filter_map(|prefix| {
            path.strip_prefix(*prefix)
                .filter(|suffix| suffix.is_empty() || suffix.starts_with('/'))
        })
        .max_by_key(|suffix| path.len() - suffix.len());

    match matched {
        Some(suffix) => Cow::Owned(format!("/api/v0{suffix}")),
        None => Cow::Borrowed(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A route registered without a row would enforce the safe default on a
    /// privileged path, and would be missing from the generated OpenAPI
    /// document. Axum enumerates nothing, so scan the source instead: every
    /// `.route()` and `.nest()` literal must appear in some row. `/` is the
    /// nested index route, which every nest prefix implies.
    #[test]
    fn every_registered_route_has_a_row() {
        let source = include_str!("router/routes.rs");
        let mut literals = Vec::new();
        for marker in [".route(\n", ".route(", ".nest("] {
            let mut rest = source;
            while let Some(at) = rest.find(marker) {
                rest = &rest[at + marker.len()..];
                let Some(open) = rest.find('"') else { break };
                let after = &rest[open + 1..];
                let Some(close) = after.find('"') else { break };
                let literal = &after[..close];
                if literal.starts_with('/') {
                    literals.push(literal.to_string());
                }
            }
        }
        assert!(
            literals.len() > 50,
            "the scan found only {} paths",
            literals.len()
        );

        for literal in literals {
            if literal == "/" {
                continue;
            }
            let nested = format!("{literal}/");
            assert!(
                ROUTES
                    .iter()
                    .any(|row| row.path.ends_with(&literal) || row.path.contains(&nested)),
                "routes.rs registers {literal} and no row in ROUTES covers it"
            );
        }
    }

    #[test]
    fn no_row_appears_twice() {
        for (index, row) in ROUTES.iter().enumerate() {
            for other in &ROUTES[index + 1..] {
                assert!(
                    row.path != other.path || row.method != other.method,
                    "{} {:?} is listed twice",
                    row.path,
                    row.method
                );
            }
        }
    }

    /// Only the GraphQL websocket takes every method; every other row names
    /// one, so the document can list an operation per method.
    #[test]
    fn only_the_websocket_route_takes_every_method() {
        for row in ROUTES {
            assert_eq!(
                row.method.is_none(),
                row.path == "/api/v0/graphql/ws",
                "{} has an unexpected method wildcard",
                row.path
            );
        }
    }
}

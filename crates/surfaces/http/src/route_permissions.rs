//! The routes the server serves, and the permission each one needs.
//!
//! [`ROUTES`] is the one list: the auth middleware reads a route's permission
//! from it, `tests/route_permissions.rs` proves every row resolves in the
//! router, and the OpenAPI document will be generated from it (#3263).
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
    let exact = ROUTES
        .iter()
        .find(|candidate| candidate.path == path && candidate.matches(method));
    if exact.is_some() {
        return exact;
    }
    // `Router::get` serves HEAD as well, with the response body removed, and the
    // method reaches the middleware unchanged. An implicit HEAD is therefore
    // authorized as the GET it runs; an explicit HEAD row still wins above.
    if method == Method::HEAD {
        return ROUTES
            .iter()
            .find(|candidate| candidate.path == path && candidate.matches(&Method::GET));
    }
    None
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
    /// document. Axum enumerates nothing, so read `router/routes.rs`: resolve
    /// each `.route()` through its enclosing `.nest()` chain and require the
    /// `(path, method)` pair to be a row.
    #[test]
    fn every_registered_route_has_a_row() {
        let missing = uncovered(include_str!("router/routes.rs"), ROUTES);
        assert!(
            missing.is_empty(),
            "these registrations have no row in ROUTES: {missing:?}"
        );
    }

    /// The scan reads text, so it needs its own proof. Each case withholds one
    /// row from the set the scan checks against: a scan that stopped finding
    /// registrations, or that matched on a shared suffix, would pass all three
    /// with the rows intact and fail none of them here.
    #[test]
    fn the_scan_names_a_row_that_is_missing() {
        let source = include_str!("router/routes.rs");
        let without = |path: &str, method: &str| -> Vec<Route> {
            ROUTES
                .iter()
                .filter(|row| {
                    !(row.path == path && row.method.as_ref().map(Method::as_str) == Some(method))
                })
                .cloned()
                .collect()
        };

        // A method on a path that keeps two others.
        assert_eq!(
            uncovered(source, &without("/api/v0/p2p/replicators", "DELETE")),
            vec!["DELETE /api/v0/p2p/replicators".to_string()]
        );

        // A path whose suffix another row also ends with: `/acp/status` must not
        // cover `/acp/node/status`.
        assert_eq!(
            uncovered(source, &without("/api/v0/acp/node/status", "GET")),
            vec!["GET /api/v0/acp/node/status".to_string()]
        );

        // A route on the root mount, which is not nested at `/api/v0`.
        assert_eq!(
            uncovered(source, &without("/health-check", "GET")),
            vec!["GET /health-check".to_string()]
        );
    }

    /// A router the scan cannot place is reported, so a new `.nest()` chain
    /// cannot be added and silently skipped.
    #[test]
    fn the_scan_reports_a_router_it_cannot_place() {
        let source =
            include_str!("router/routes.rs").replace(".nest(\"/acp/node\", acp_node_routes)", "");
        let missing = uncovered(&source, ROUTES);
        assert!(
            missing
                .iter()
                .any(|entry| entry.contains("acp_node_routes is nested in no router")),
            "an orphaned router must be reported, got {missing:?}"
        );
    }

    /// Every `(path, method)` pair `router/routes.rs` registers and no row
    /// covers. A router this scan cannot place is reported rather than skipped,
    /// so a new one cannot be added and quietly unchecked.
    fn uncovered(source: &str, rows: &[Route]) -> Vec<String> {
        let source = match source.find("#[cfg(test)]") {
            Some(end) => &source[..end],
            None => source,
        };
        let routers = routers(source);

        // (child, parent, prefix) for every `.nest()` between two routers.
        let mut mounts: Vec<(String, String, String)> = Vec::new();
        for (parent, body) in &routers {
            for (prefix, child) in nest_calls(body) {
                mounts.push((child, parent.clone(), prefix));
            }
        }

        let mut missing = Vec::new();
        for (router, body) in &routers {
            let Some(base) = base_of(router, &mounts, 0) else {
                missing.push(format!(
                    "{router} is nested in no router this scan can resolve"
                ));
                continue;
            };
            for (literal, methods) in route_calls(body) {
                let path = if literal == "/" {
                    base.clone()
                } else {
                    format!("{base}{literal}")
                };
                for method in &methods {
                    let listed = rows.iter().any(|row| {
                        row.path == path
                            && match method {
                                None => row.method.is_none(),
                                Some(method) => row.method.as_ref() == Some(method),
                            }
                    });
                    if !listed {
                        let name = match method {
                            None => "ANY".to_string(),
                            Some(method) => method.as_str().to_string(),
                        };
                        missing.push(format!("{name} {path}"));
                    }
                }
            }
        }
        missing
    }

    /// The path a router's `/` resolves to. `api_routes` is mounted by the
    /// `fold` over `go_paths::API_PREFIXES` rather than by a `.nest()` call, and
    /// every row carries the first prefix's form.
    fn base_of(router: &str, mounts: &[(String, String, String)], depth: usize) -> Option<String> {
        match router {
            "root_routes" => return Some(String::new()),
            "api_routes" => return Some(crate::go_paths::API_PREFIXES[0].to_string()),
            _ => {}
        }
        if depth > 8 {
            return None;
        }
        let mut resolved: Option<String> = None;
        for (_, parent, prefix) in mounts.iter().filter(|(child, _, _)| child == router) {
            let base = base_of(parent, mounts, depth + 1)?;
            if resolved.is_some() {
                // Nested twice: one path cannot be named for both.
                return None;
            }
            resolved = Some(format!("{base}{prefix}"));
        }
        resolved
    }

    /// Each `let NAME = Router::new()` and the text of its chain.
    fn routers(source: &str) -> Vec<(String, &str)> {
        let mut found = Vec::new();
        let mut offset = 0;
        let mut start = None;
        for line in source.split_inclusive('\n') {
            let trimmed = line.trim_start();
            let is_router = trimmed.starts_with("let ") && trimmed.contains("= Router::new()");
            if is_router {
                if let Some((name, start_offset)) = start {
                    found.push((name, &source[start_offset..offset]));
                }
                let name = trimmed
                    .trim_start_matches("let ")
                    .split_whitespace()
                    .next()
                    .expect("a binding name")
                    .to_string();
                start = Some((name, offset));
            }
            offset += line.len();
        }
        if let Some((name, start_offset)) = start {
            found.push((name, &source[start_offset..]));
        }
        found
    }

    /// Every `.nest("PREFIX", CHILD)` call, with `.clone()` trimmed. The one
    /// mount that names a variable rather than a literal is the `fold` over
    /// `API_PREFIXES`, which `uncovered` handles as `api_routes`' base.
    fn nest_calls(source: &str) -> Vec<(String, String)> {
        let mut found = Vec::new();
        let mut rest = source;
        while let Some(at) = rest.find(".nest(") {
            rest = &rest[at + ".nest(".len()..];
            let Some(prefix) = string_literal(rest) else {
                continue;
            };
            let args = &rest[prefix.1..];
            let child = args
                .trim_start_matches([',', ' ', '\t', '\n'])
                .split([',', ')'])
                .next()
                .unwrap_or_default()
                .trim()
                .trim_end_matches(".clone()");
            if !prefix.0.is_empty() && !child.is_empty() {
                found.push((prefix.0.to_string(), child.to_string()));
            }
        }
        found
    }

    /// Every `.route("LITERAL", <methods>)` in `body`.
    fn route_calls(body: &str) -> Vec<(String, Vec<Option<Method>>)> {
        let mut found = Vec::new();
        let mut rest = body;
        while let Some(at) = rest.find(".route(") {
            rest = &rest[at + ".route(".len()..];
            let Some(literal) = string_literal(rest) else {
                continue;
            };
            let (args, _) = call_arguments(&rest[literal.1..]);
            let mut methods = Vec::new();
            for name in method_names(&args) {
                match name {
                    "get" => methods.push(Some(Method::GET)),
                    "post" => methods.push(Some(Method::POST)),
                    "put" => methods.push(Some(Method::PUT)),
                    "patch" => methods.push(Some(Method::PATCH)),
                    "delete" => methods.push(Some(Method::DELETE)),
                    "head" => methods.push(Some(Method::HEAD)),
                    "options" => methods.push(Some(Method::OPTIONS)),
                    "any" => methods.push(None),
                    _ => {}
                }
            }
            found.push((literal.0.to_string(), methods));
        }
        found
    }

    /// The string literal `source` starts with, after any whitespace, and the
    /// offset just past it.
    fn string_literal(source: &str) -> Option<(&str, usize)> {
        let trimmed = source.trim_start();
        let after = trimmed.strip_prefix('"')?;
        let close = after.find('"')?;
        let offset = source.len() - trimmed.len() + 1 + close + 1;
        Some((&after[..close], offset))
    }

    /// The identifiers immediately before each `(` in a route's arguments, so
    /// `get(a).patch(b).merge(capped(post(c)))` names get, patch, capped and
    /// post.
    fn method_names(args: &str) -> Vec<&str> {
        let mut names = Vec::new();
        let mut rest = args;
        while let Some(at) = rest.find('(') {
            let before = &rest[..at];
            let name = before
                .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .next()
                .unwrap_or_default();
            names.push(name);
            rest = &rest[at + 1..];
        }
        names
    }

    /// The text between a call's opening parenthesis and its match, plus what
    /// follows the close. `source` starts just after the opening parenthesis.
    fn call_arguments(source: &str) -> (String, String) {
        let mut depth = 1;
        for (index, character) in source.char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return (source[..index].to_string(), source[index + 1..].to_string());
                    }
                }
                _ => {}
            }
        }
        (source.to_string(), String::new())
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

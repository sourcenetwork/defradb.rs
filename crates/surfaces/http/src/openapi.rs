//! The OpenAPI document generated from [`ROUTES`].
//!
//! Go's pinned document (`handlers::openapi::OPENAPI_GO`) is the base:
//! every operation it describes is carried verbatim, so a client generated
//! from this document still gets Go's request and response schemas. On top of
//! that, every route in [`ROUTES`] that Go does not document gets an
//! operation, which is what makes the served document true of this server
//! instead of only of Go (#3263).
//!
//! Two things the generated operations do not have yet, because the table
//! carries no request or response types: a request body, and a typed success
//! response. Both say so in the document rather than staying silent. The error
//! response is Go's `#/components/responses/error`, so the envelope is right
//! even while the bodies are not.
//!
//! The document declares [`OPENAPI_VERSION`]; Go's is 3.0.3 and its schemas are
//! written in that dialect, so this one is too until the schemas are derived
//! here as well.
//!
//! Because Go's operations are carried verbatim, the `x-defra-permission`
//! extension appears only on the operations generated here: adding it to Go's
//! would mean rewriting them, and the parity of Go's 64 operations is worth
//! more than a uniform extension. The permission table is the authority for the
//! rest.

use serde_json::{json, Map, Value};

use crate::handlers::openapi::OPENAPI_GO;
use crate::route_permissions::{Route, RoutePermission, ROUTES};

/// The version the document declares: the same as Go's, because the operations
/// it carries are Go's own and its schemas are written in that dialect.
pub const OPENAPI_VERSION: &str = "3.0.3";

/// The document: Go's operations where Go documents the path, `ROUTES`
/// everywhere else.
///
/// Built per request. This endpoint is read by a client generating bindings,
/// not on a data path, so a parse of the pinned document is cheaper than the
/// shared cache that holding it would need.
pub fn document() -> Value {
    let mut doc: Value = serde_json::from_str(OPENAPI_GO).expect("the pinned Go document parses");
    let mut paths = doc["paths"].take();
    {
        let paths = paths
            .as_object_mut()
            .expect("the pinned Go document has a paths object");

        for row in ROUTES {
            let path = documented_path(row);
            let method = documented_method(row);
            let item = paths
                .entry(path.clone())
                .or_insert_with(|| Value::Object(Map::new()));

            // Go's operation stays exactly as it is: it carries the schemas
            // this document cannot derive yet.
            if item.get(&method).is_some() {
                continue;
            }
            item[method.as_str()] = operation(row, &path, &method);

            // Only the root mount serves these, so the operation cannot inherit
            // the document's `/api/v0` servers.
            if path == row.path {
                item["servers"] = json!([{ "url": "/" }]);
            }
        }
    }
    doc["paths"] = paths;
    doc["openapi"] = json!(OPENAPI_VERSION);
    add_missing_tags(&mut doc);
    doc
}

/// The path as the document spells it: Go's document writes paths relative to
/// its `servers`, which mount `/api/v0`, `/api/v1` and `/api`.
fn documented_path(row: &Route) -> String {
    match row.path.strip_prefix("/api/v0") {
        Some(rest) => rest.to_string(),
        None => row.path.to_string(),
    }
}

/// The method the document lists for a row. The GraphQL websocket is
/// registered with `any()`, which OpenAPI cannot express; it is documented as
/// `get`, the method a websocket upgrade arrives as.
fn documented_method(row: &Route) -> String {
    match &row.method {
        Some(method) => method.as_str().to_ascii_lowercase(),
        None => "get".to_string(),
    }
}

fn operation(row: &Route, path: &str, method: &str) -> Value {
    let mut operation = json!({
        "operationId": operation_id(method, path),
        "tags": [tag_name(path)],
        "description": description(path),
        "security": security(row),
        "responses": {
            "200": {
                "description": "The operation's result. No response schema is derived yet (#3263)."
            },
            "default": { "$ref": "#/components/responses/error" }
        },
        "x-defra-permission": permission_name(&row.permission),
    });

    responses_for(path, &mut operation);

    let parameters = path_parameters(path);
    if !parameters.is_empty() {
        operation["parameters"] = Value::Array(parameters);
    }
    operation
}

/// The routes whose response is not the `200` the default block declares.
///
/// Both are GraphQL transports a plain request/response consumer would get
/// wrong: the signer upgrades the connection, and the websocket route has no
/// implementation behind it yet.
fn responses_for(path: &str, operation: &mut Value) {
    match path.strip_prefix("/api/v0").unwrap_or(path) {
        "/graphql/client-sign" => {
            operation["responses"]["101"] = json!({
                "description": "Switching Protocols: the connection became a WebSocket."
            });
        }
        "/graphql/ws" => {
            operation["responses"] = json!({
                "501": {
                    "description": "Not Implemented: subscriptions over a WebSocket are not served yet."
                },
                "default": { "$ref": "#/components/responses/error" }
            });
        }
        _ => {}
    }
}

fn description(path: &str) -> &'static str {
    if path.strip_prefix("/api/v0").unwrap_or(path) == "/graphql/ws" {
        "GraphQL subscriptions over a WebSocket upgrade, which this server does not serve yet: the route answers 501 Not Implemented. Any method is accepted."
    } else {
        "Served by this node. No description or request body is derived yet (#3263)."
    }
}

/// `bearerToken` is the scheme Go's document already defines; an exempt route
/// declares no security, which is how OpenAPI spells "nothing required".
///
/// A `Dynamic` route decides for itself and runs unauthenticated when DAC and
/// NAC are both off, so it declares the scheme as optional — the same
/// `[{}, {bearerToken: []}]` Go's document puts at the root.
///
/// `Required` and `IdentityOnly` fit the same shape for a different reason: the
/// permission is checked *against a caller*, and with NAC off there is no check
/// to fail, so the request is served without a token. Only `IdentityOnly`
/// routes that cannot act without an identity — and `Dynamic` handlers that
/// reject an unidentified caller, which is what `caller_required` marks — need
/// the token unconditionally.
fn security(row: &Route) -> Value {
    if row.caller_required || row.permission == RoutePermission::IdentityOnly {
        return json!([{ "bearerToken": [] }]);
    }
    match row.permission {
        RoutePermission::Exempt => json!([]),
        RoutePermission::Dynamic | RoutePermission::IdentityOnly | RoutePermission::Required(_) => {
            json!([{}, { "bearerToken": [] }])
        }
    }
}

/// What the edge enforces, as the permission table names it, so a reader can
/// tell a route that decides in the handler from one that gates on a node
/// permission.
fn permission_name(permission: &RoutePermission) -> String {
    match permission {
        RoutePermission::Exempt => "exempt".to_string(),
        RoutePermission::IdentityOnly => "identity-only".to_string(),
        RoutePermission::Dynamic => "dynamic".to_string(),
        RoutePermission::Required(required) => serde_json::to_value(required)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "required".to_string()),
    }
}

/// `{name}` segments, declared as required string path parameters: a document
/// whose path template lists a parameter it never declares is invalid.
fn path_parameters(path: &str) -> Vec<Value> {
    let mut parameters = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let name = &rest[open + 1..open + close];
        parameters.push(json!({
            "name": name,
            "in": "path",
            "required": true,
            "schema": { "type": "string" }
        }));
        rest = &rest[open + close + 1..];
    }
    parameters
}

/// `get_collections_name_exists`, stable across runs and readable in a
/// generated client. A route that names its handler in the table can replace
/// this with the handler's own name.
fn operation_id(method: &str, path: &str) -> String {
    let mut id = format!("{method}_");
    let mut separator = true;
    for character in path.chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator {
            id.push('_');
            separator = true;
        }
    }
    id.trim_end_matches('_').to_string()
}

/// The first path segment, under the names Go's document already tags with.
///
/// The routes that describe or configure the node itself (`/health-check`,
/// both documents, `/version`, `/debug/dump`) are grouped under `server`
/// rather than each becoming a tag with one operation.
fn tag_name(path: &str) -> String {
    let segment = path
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or_default();
    match segment {
        "collections" => "collection".to_string(),
        "tx" => "transaction".to_string(),
        "views" => "view".to_string(),
        "health-check" | "openapi.json" | "openapi.rs.json" | "version" | "debug" | "node" => {
            "server".to_string()
        }
        other => other.to_string(),
    }
}

/// Every tag an operation names needs an entry, or a renderer drops it.
fn add_missing_tags(doc: &mut Value) {
    let used: std::collections::BTreeSet<String> = doc["paths"]
        .as_object()
        .map(|paths| {
            paths
                .values()
                .flat_map(|item| item.as_object().into_iter().flatten())
                .filter(|(method, _)| is_operation(method))
                .flat_map(|(_, operation)| {
                    operation["tags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|tag| tag.as_str().map(str::to_string))
                })
                .collect()
        })
        .unwrap_or_default();

    let declared: Vec<String> = doc["tags"]
        .as_array()
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| tag["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    for tag in used {
        if declared.contains(&tag) {
            continue;
        }
        let tags = doc["tags"].as_array_mut().expect("tags is an array");
        tags.push(json!({ "name": tag }));
    }
}

fn is_operation(key: &str) -> bool {
    matches!(
        key,
        "get" | "put" | "post" | "delete" | "options" | "head" | "patch" | "trace"
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// `(path, method)` pairs the document lists, in the table's spelling.
    fn documented(doc: &Value) -> BTreeSet<(String, String)> {
        let mut pairs = BTreeSet::new();
        for (path, item) in doc["paths"].as_object().expect("paths") {
            let rooted = item["servers"]
                .as_array()
                .is_some_and(|servers| servers.iter().any(|server| server["url"] == "/"));
            let path = if rooted || path.starts_with("/api/") {
                path.clone()
            } else {
                format!("/api/v0{path}")
            };
            for method in item.as_object().expect("path item").keys() {
                if is_operation(method) {
                    pairs.insert((path.clone(), method.clone()));
                }
            }
        }
        pairs
    }

    /// `(path, method)` pairs `ROUTES` serves, in the document's spelling.
    fn served() -> BTreeSet<(String, String)> {
        ROUTES
            .iter()
            .map(|row| (row.path.to_string(), documented_method(row)))
            .collect()
    }

    /// Every route this server serves is in the document. This is what #3263 is
    /// about: the document used to describe Go and not this server.
    #[test]
    #[mantra_macros::req_test("defra.http.openapi")]
    fn every_route_in_the_table_is_documented() {
        let document = document();
        let listed = documented(&document);
        for row in ROUTES {
            let method = documented_method(row);
            assert!(
                listed.contains(&(row.path.to_string(), method.clone())),
                "{} {method} is served and not documented",
                row.path
            );
        }
    }

    /// And the other way: the document lists nothing this server does not
    /// serve, so a generated client cannot call a 404.
    #[test]
    fn every_documented_operation_is_a_route_in_the_table() {
        let listed = documented(&document());
        let served = served();
        for pair in &listed {
            assert!(
                served.contains(pair),
                "the document lists {} {} and ROUTES does not",
                pair.0,
                pair.1
            );
        }
    }

    /// Go's operations are carried untouched, schemas included, so a client
    /// generated from this document is at least as complete as one generated
    /// from Go's. A path item may gain methods Go does not document (the
    /// relationship routes gain `get`); the operations Go does document must
    /// come out identical.
    #[test]
    fn gos_operations_are_carried_verbatim() {
        let pinned: Value = serde_json::from_str(OPENAPI_GO).expect("pinned");
        let document = document();
        let mut compared = 0;
        for (path, item) in pinned["paths"].as_object().expect("paths") {
            for (method, operation) in item.as_object().expect("path item") {
                if !is_operation(method) {
                    continue;
                }
                assert_eq!(
                    &document["paths"][path][method], operation,
                    "the generated document rewrote Go's {method} {path}"
                );
                compared += 1;
            }
        }
        let operations = pinned["paths"]
            .as_object()
            .expect("paths")
            .values()
            .map(|item| {
                item.as_object()
                    .expect("path item")
                    .keys()
                    .filter(|key| is_operation(key))
                    .count()
            })
            .sum::<usize>();
        assert_eq!(compared, operations, "every operation of Go's document");
    }

    /// A path template that names a parameter without declaring it is not a
    /// valid document.
    #[test]
    fn every_generated_path_parameter_is_declared() {
        let document = document();
        for (path, item) in document["paths"].as_object().expect("paths") {
            for (method, operation) in item.as_object().expect("path item") {
                if !is_operation(method) {
                    continue;
                }
                let declared: BTreeSet<&str> = operation["parameters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|parameter| parameter["name"].as_str())
                    .collect();
                let mut rest = path.as_str();
                while let Some(open) = rest.find('{') {
                    let close = rest[open..].find('}').expect("closed parameter");
                    let name = &rest[open + 1..open + close];
                    assert!(
                        declared.contains(name),
                        "{method} {path} names {{{name}}} and does not declare it"
                    );
                    rest = &rest[open + close + 1..];
                }
            }
        }
    }

    /// The envelope and the mounts are Go's, so a client's error handling and
    /// base URL are unchanged.
    #[test]
    fn the_envelope_and_the_mounts_are_gos() {
        let pinned: Value = serde_json::from_str(OPENAPI_GO).expect("pinned");
        let document = document();
        assert_eq!(document["openapi"], OPENAPI_VERSION);
        assert_eq!(document["info"], pinned["info"]);
        assert_eq!(document["servers"], pinned["servers"]);
        assert_eq!(
            document["components"]["securitySchemes"],
            pinned["components"]["securitySchemes"]
        );
        assert!(document["components"]["responses"]["error"].is_object());
    }

    /// The edge's permission travels with a generated operation, which is what a
    /// reader needs to know before wiring a client. The paths here are the ones
    /// Go does not document, because Go's operations are carried verbatim.
    #[test]
    fn the_edge_permission_travels_with_the_operation() {
        let document = document();
        assert_eq!(
            document["paths"]["/p2p/manage"]["post"]["x-defra-permission"],
            "P2pPeerConnect"
        );
        assert_eq!(
            document["paths"]["/batch/start"]["post"]["x-defra-permission"],
            "dynamic"
        );
        assert_eq!(
            document["paths"]["/version"]["get"]["x-defra-permission"],
            "exempt"
        );
        assert_eq!(document["paths"]["/version"]["get"]["security"], json!([]));
        assert_eq!(
            document["paths"]["/p2p/manage"]["post"]["security"],
            json!([{}, { "bearerToken": [] }])
        );
    }

    /// A `Dynamic` route decides for itself and runs with no caller when DAC and
    /// NAC are off, so it declares the scheme as optional. A node permission is
    /// checked against a caller and there is no check to fail while NAC is off,
    /// so it is optional for the same reason. Only a handler that rejects a
    /// caller it cannot identify declares the scheme required.
    #[test]
    #[mantra_macros::req_test("defra.http.openapi")]
    fn security_matches_what_the_edge_actually_requires() {
        let document = document();
        assert_eq!(
            document["paths"]["/batch/start"]["post"]["security"],
            json!([{}, { "bearerToken": [] }]),
            "a dynamic route is reachable without a caller"
        );
        assert_eq!(
            document["paths"]["/p2p/manage"]["post"]["security"],
            json!([{}, { "bearerToken": [] }]),
            "a node permission is not checked while NAC is off"
        );
        assert_eq!(
            document["paths"]["/acp/node/enable"]["post"]["security"],
            json!([{ "bearerToken": [] }]),
            "the handler rejects an unidentified caller"
        );
    }

    /// Every tag an operation names is declared, not only the first one: Go's
    /// `/node/identity` carries `["node", "identity"]`.
    #[test]
    fn every_tag_an_operation_uses_is_declared() {
        let document = document();
        let declared: BTreeSet<&str> = document["tags"]
            .as_array()
            .expect("tags")
            .iter()
            .filter_map(|tag| tag["name"].as_str())
            .collect();
        for (path, item) in document["paths"].as_object().expect("paths") {
            for (method, operation) in item.as_object().expect("path item") {
                if !is_operation(method) {
                    continue;
                }
                for tag in operation["tags"].as_array().into_iter().flatten() {
                    let tag = tag.as_str().expect("a tag name");
                    assert!(
                        declared.contains(tag),
                        "{method} {path} is tagged {tag} and the document does not declare it"
                    );
                }
            }
        }
    }

    /// `operationId` is what a client generator names its methods after, so it
    /// must be unique across the document.
    #[test]
    fn operation_ids_are_unique() {
        let document = document();
        let mut seen = BTreeSet::new();
        for item in document["paths"].as_object().expect("paths").values() {
            for (method, operation) in item.as_object().expect("path item") {
                if !is_operation(method) {
                    continue;
                }
                if let Some(id) = operation["operationId"].as_str() {
                    assert!(
                        seen.insert(id.to_string()),
                        "operationId {id} is used twice"
                    );
                }
            }
        }
    }
}
#[cfg(test)]
mod response_and_security_tests {
    use super::*;

    /// The operation this server generates for `method path`, or `None` when
    /// Go's document supplies it and the row is carried verbatim.
    fn generated(method: &str, path: &str) -> Option<Value> {
        let row = crate::route_table::ROUTES.iter().find(|row| {
            row.path == path
                && row
                    .method
                    .as_ref()
                    .is_none_or(|own| own.as_str().eq_ignore_ascii_case(method))
        })?;
        Some(operation(row, path, method))
    }

    #[test]
    fn the_signing_upgrade_declares_switching_protocols() {
        let operation = generated("get", "/api/v0/graphql/client-sign").expect("generated");
        assert!(
            operation["responses"]["101"].is_object(),
            "a successful sign upgrades the connection: {operation}"
        );
    }

    #[test]
    fn the_websocket_route_declares_not_implemented() {
        let operation = generated("get", "/api/v0/graphql/ws").expect("generated");
        assert!(
            operation["responses"]["200"].is_null(),
            "nothing implements it, so no success is advertised: {operation}"
        );
        assert!(
            operation["responses"]["501"].is_object(),
            "the handler answers 501: {operation}"
        );
        assert!(
            operation["description"]
                .as_str()
                .is_some_and(|text| text.contains("501")),
            "the description says the same: {operation}"
        );
    }

    #[test]
    fn a_row_that_needs_a_caller_requires_bearer() {
        for path in [
            "/api/v0/acp/node/enable",
            "/api/v0/acp/node/source-permission",
        ] {
            let operation = generated("post", path).expect("generated");
            assert_eq!(
                operation["security"],
                json!([{ "bearerToken": [] }]),
                "{path} rejects a caller it cannot identify"
            );
        }
    }

    #[test]
    fn a_node_permission_declares_bearer_as_available_not_required() {
        let operation = generated("get", "/api/v0/p2p/shareable-address").expect("generated");
        assert_eq!(
            operation["security"],
            json!([{}, { "bearerToken": [] }]),
            "with NAC off there is no check to fail, so the token is optional"
        );
    }
}

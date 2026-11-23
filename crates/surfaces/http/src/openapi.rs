//! The OpenAPI document generated from [`ROUTES`].
//!
//! Go's pinned document (`handlers/openapi-go-307364d9d.json`) is the base:
//! every operation it describes is carried verbatim, so a client generated
//! from this document still gets Go's request and response schemas. On top of
//! that, every route in [`ROUTES`] that Go does not document gets an
//! operation, which is what makes the served document true of this server
//! instead of only of Go (#3263).
//!
//! A row may name the JSON body it accepts and the one it returns
//! (`Route::request`, `Route::response`). Those are rendered with one
//! [`SchemaGenerator`], so a type two routes share is one component, and every
//! operation references components by name rather than carrying an anonymous
//! copy. A route that names neither says so in its description rather than
//! staying silent, so what is still undescribed is visible in the document
//! itself.
//!
//! The error response is Go's `#/components/responses/error`, so the envelope
//! is right even for the routes whose bodies are not derived yet.
//!
//! The document declares [`OPENAPI_VERSION`]: 3.0.3, Go's version, because the
//! operations it carries are Go's and its schemas are written in that dialect.
//! `schemars` derives JSON Schema 2020-12, so [`to_openapi_30`] rewrites what
//! it emits into the declared dialect rather than mixing the two.
//!
//! Because Go's operations are carried verbatim, the `x-defra-permission`
//! extension appears only on the operations generated here: adding it to Go's
//! would mean rewriting them, and the parity of Go's 64 operations is worth
//! more than a uniform extension. The permission table is the authority for the
//! rest.

use schemars::SchemaGenerator;
use serde_json::{json, Map, Value};

use crate::route_permissions::{Route, RoutePermission, ROUTES};
use crate::route_table::BodySchema;

/// The version the document declares: the same as Go's, because the operations
/// it carries are Go's own and its schemas are written in that dialect.
pub const OPENAPI_VERSION: &str = "3.0.3";

/// Go's document, carried as the base. Refreshed with the Go baseline, which
/// `handlers/openapi.rs` pins.
const PINNED_GO_DOCUMENT: &str = include_str!("handlers/openapi-go-307364d9d.json");

/// The document: Go's operations where Go documents the path, `ROUTES`
/// everywhere else.
///
/// Built per request. This endpoint is read by a client generating bindings,
/// not on a data path, so a parse of the pinned document is cheaper than the
/// shared cache that holding it would need.
pub fn document() -> Value {
    let mut doc: Value =
        serde_json::from_str(PINNED_GO_DOCUMENT).expect("the pinned Go document parses");
    let mut generator = SchemaGenerator::default();
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
            let request = body_schema(row.request, &mut generator);
            let response = body_schema(row.response, &mut generator);
            item[method.as_str()] = operation(row, &path, &method, request, response);

            // Only the root mount serves these, so the operation cannot inherit
            // the document's `/api/v0` servers.
            if path == row.path {
                item["servers"] = json!([{ "url": "/" }]);
            }
        }
    }
    doc["paths"] = paths;
    doc["openapi"] = json!(OPENAPI_VERSION);
    publish_derived_schemas(&mut doc, &generator);
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

fn operation(
    row: &Route,
    path: &str,
    method: &str,
    request: Option<Value>,
    response: Option<Value>,
) -> Value {
    let described_result = response.is_some();
    let mut operation = json!({
        "operationId": operation_id(method, path),
        "tags": [tag_name(path)],
        "description": description(path),
        "security": security(&row.permission),
        "responses": {
            "200": {
                "description": if described_result {
                    "The operation's result."
                } else {
                    "The operation's result. No response schema is derived yet (#3263)."
                }
            },
            "default": { "$ref": "#/components/responses/error" }
        },
        "x-defra-permission": permission_name(&row.permission),
    });

    if let Some(request) = request {
        operation["requestBody"] = json!({
            "required": row.request_required,
            "content": { "application/json": { "schema": request } }
        });
    }
    if let Some(response) = response {
        operation["responses"]["200"]["content"] =
            json!({ "application/json": { "schema": response } });
    }

    let parameters = path_parameters(path);
    if !parameters.is_empty() {
        operation["parameters"] = Value::Array(parameters);
    }
    operation
}

/// One body, rendered into `components.schemas` so every operation references
/// a named schema rather than carrying an anonymous copy of it.
fn body_schema(render: Option<BodySchema>, generator: &mut SchemaGenerator) -> Option<Value> {
    let render = render?;
    let mut value = serde_json::to_value(render(generator)).expect("a schemars schema serializes");
    to_openapi_30(&mut value);
    let name = value
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string)?;
    if let Value::Object(map) = &mut value {
        map.remove("title");
        map.remove("$defs");
    }
    generator
        .definitions_mut()
        .entry(name.clone())
        .or_insert(value);
    Some(json!({ "$ref": format!("#/components/schemas/{name}") }))
}

/// Puts every schema the typed rows reached into `components.schemas`.
///
/// `schemars` collects them in one generator, so a type two routes share is one
/// component. `$ref`s are pointed at the document's `components` by
/// [`to_openapi_30`].
fn publish_derived_schemas(doc: &mut Value, generator: &SchemaGenerator) {
    if generator.definitions().is_empty() {
        return;
    }
    let mut derived =
        serde_json::to_value(generator.definitions()).expect("schemars definitions serialize");
    to_openapi_30(&mut derived);
    let schemas = doc["components"]["schemas"]
        .as_object_mut()
        .expect("Go's document has component schemas");
    for (name, schema) in derived.as_object().into_iter().flatten() {
        assert!(
            !schemas.contains_key(name),
            "the derived schema {name} shadows one of Go's"
        );
        schemas.insert(name.clone(), schema.clone());
    }
}

/// Rewrites a JSON Schema 2020-12 (what `schemars` derives) into the OpenAPI
/// 3.0 dialect this document declares, and points `$ref`s at its components.
///
/// 3.0 has no `null` type, no boolean schemas, no `const`, and treats
/// `exclusiveMinimum` as a flag rather than a number. A reader that validates
/// the document rejects a document that mixes the two.
fn to_openapi_30(value: &mut Value) {
    match value {
        // Left alone: a boolean here is data (`nullable`, an `enum` value). The
        // boolean *schemas* are normalised by the object arm, which knows the
        // keys that hold one.
        Value::Bool(_) => {}
        Value::Array(items) => items.iter_mut().for_each(to_openapi_30),
        Value::Object(map) => {
            // Only these keys hold a schema (or a list of them); `enum` and
            // `required` hold data, and `nullable` is a flag.
            // `additionalProperties` is not here: a boolean is valid in 3.0's
            // Draft-4 dialect, and `deny_unknown_fields` means `false`.
            for key in ["items", "not", "additionalItems", "contains"] {
                if matches!(map.get(key), Some(Value::Bool(_))) {
                    map.insert(key.to_string(), Value::Object(Map::new()));
                }
            }
            for key in ["properties", "$defs", "definitions"] {
                if let Some(Value::Object(children)) = map.get_mut(key) {
                    for child in children.values_mut() {
                        if matches!(child, Value::Bool(_)) {
                            *child = Value::Object(Map::new());
                        }
                    }
                }
            }
            for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
                if let Some(Value::Array(branches)) = map.get_mut(key) {
                    for branch in branches.iter_mut() {
                        if matches!(branch, Value::Bool(_)) {
                            *branch = Value::Object(Map::new());
                        }
                    }
                }
            }

            map.remove("$schema");

            // `type: [T, "null"]` is `type: T, nullable: true` in 3.0.
            if let Some(Value::Array(types)) = map.get("type").cloned() {
                let nullable = types.iter().any(|kind| kind == "null");
                let mut remaining: Vec<Value> =
                    types.into_iter().filter(|kind| kind != "null").collect();
                if nullable {
                    map.insert("nullable".to_string(), Value::Bool(true));
                }
                match remaining.len() {
                    0 => {
                        map.remove("type");
                    }
                    1 => {
                        map.insert("type".to_string(), remaining.remove(0));
                    }
                    _ => {
                        map.insert("type".to_string(), Value::Array(remaining));
                    }
                }
            }

            // An `anyOf` with a null branch is the same thing spelled out.
            if let Some(Value::Array(branches)) = map.get("anyOf").cloned() {
                let (nulls, rest): (Vec<Value>, Vec<Value>) = branches
                    .into_iter()
                    .partition(|branch| branch["type"] == "null");
                if !nulls.is_empty() {
                    map.insert("nullable".to_string(), Value::Bool(true));
                    match rest.len() {
                        0 => {
                            map.remove("anyOf");
                        }
                        1 => {
                            map.remove("anyOf");
                            if let Value::Object(only) = rest.into_iter().next().expect("one") {
                                map.extend(only);
                            }
                        }
                        _ => {
                            map.insert("anyOf".to_string(), Value::Array(rest));
                        }
                    }
                }
            }

            if let Some(constant) = map.remove("const") {
                map.insert("enum".to_string(), Value::Array(vec![constant]));
            }

            for (exclusive, bound) in [
                ("exclusiveMinimum", "minimum"),
                ("exclusiveMaximum", "maximum"),
            ] {
                if matches!(map.get(exclusive), Some(Value::Number(_))) {
                    let value = map.remove(exclusive).expect("a number");
                    map.insert(bound.to_string(), value);
                    map.insert(exclusive.to_string(), Value::Bool(true));
                }
            }

            // After the merges above: a single branch lifted out of `anyOf` can
            // be the `$ref` itself.
            if let Some(Value::String(reference)) = map.get("$ref") {
                if let Some(name) = reference.strip_prefix("#/$defs/") {
                    map.insert(
                        "$ref".to_string(),
                        Value::String(format!("#/components/schemas/{name}")),
                    );
                }
            }

            for child in map.values_mut() {
                to_openapi_30(child);
            }
        }
        _ => {}
    }
}

fn description(path: &str) -> &'static str {
    if path == "/graphql/ws" {
        "GraphQL subscriptions over a WebSocket upgrade. Any method is accepted."
    } else {
        "Served by this node. No description or request body is derived yet (#3263)."
    }
}

/// `bearerToken` is the scheme Go's document already defines; an exempt route
/// declares no security, which is how OpenAPI spells "nothing required".
///
/// A `Dynamic` route decides for itself and runs unauthenticated when DAC and
/// NAC are both off, so it declares the scheme as optional — the same
/// `[{}, {bearerToken: []}]` Go's document puts at the root. `IdentityOnly` and
/// `Required` need a caller to decide anything, so they declare it required.
fn security(permission: &RoutePermission) -> Value {
    match permission {
        RoutePermission::Exempt => json!([]),
        RoutePermission::Dynamic => json!([{}, { "bearerToken": [] }]),
        RoutePermission::IdentityOnly | RoutePermission::Required(_) => {
            json!([{ "bearerToken": [] }])
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
        let pinned: Value = serde_json::from_str(PINNED_GO_DOCUMENT).expect("pinned");
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
        assert_eq!(compared, 64, "Go's document has 64 operations");
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
        let pinned: Value = serde_json::from_str(PINNED_GO_DOCUMENT).expect("pinned");
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
            json!([{ "bearerToken": [] }])
        );
    }

    /// A `Dynamic` route decides for itself and runs with no caller when DAC and
    /// NAC are off, so it declares the scheme as optional rather than required.
    /// A route that needs a caller to decide anything declares it required.
    #[test]
    fn security_matches_what_the_edge_actually_requires() {
        let document = document();
        assert_eq!(
            document["paths"]["/batch/start"]["post"]["security"],
            json!([{}, { "bearerToken": [] }]),
            "a dynamic route is reachable without a caller"
        );
        assert_eq!(
            document["paths"]["/p2p/manage"]["post"]["security"],
            json!([{ "bearerToken": [] }]),
            "a route gated on a node permission needs a caller"
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

    /// A typed row's bodies reach the document, both of them, as refs into the
    /// components this document declares.
    #[test]
    fn typed_routes_carry_their_request_and_response() {
        let document = document();
        let is_owner = &document["paths"]["/acp/document/is-owner"]["post"];
        assert_eq!(
            is_owner["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/OwnDocumentsRequest"
        );
        assert_eq!(
            is_owner["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/OwnDocumentsResponse"
        );
        assert!(document["components"]["schemas"]["OwnDocumentsRequest"].is_object());
        assert!(document["components"]["schemas"]["OwnDocumentsResponse"].is_object());

        let signed = &document["paths"]["/block/signed"]["get"];
        assert_eq!(
            signed["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/SignedBlockResponse"
        );
        assert!(signed["requestBody"].is_null());

        let gc = &document["paths"]["/views/gc"]["post"];
        assert_eq!(
            gc["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ViewNamesRequest"
        );
    }

    /// Every `$ref` in the document resolves. A derived schema that points at a
    /// definition the generator did not emit is a broken client generator.
    #[test]
    fn every_reference_resolves() {
        let document = document();
        let mut refs = BTreeSet::new();
        collect_refs(&document, &mut refs);
        assert!(!refs.is_empty(), "the document has references");
        for reference in refs {
            let path = reference
                .strip_prefix("#/")
                .unwrap_or_else(|| panic!("{reference} is not a local reference"))
                .split('/');
            let mut target = &document;
            for segment in path {
                target = &target[segment];
            }
            assert!(!target.is_null(), "{reference} does not resolve");
        }
    }

    fn collect_refs(value: &Value, into: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
                    into.insert(reference.to_string());
                }
                for child in map.values() {
                    collect_refs(child, into);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| collect_refs(item, into)),
            _ => {}
        }
    }

    /// The document declares 3.0.3, so no generated schema may carry a 3.1
    /// construct: `schemars` derives JSON Schema 2020-12, which has `null` in
    /// `type`, boolean schemas and `const`.
    #[test]
    fn derived_schemas_are_in_the_declared_dialect() {
        let document = document();
        let schemas = document["components"]["schemas"]
            .as_object()
            .expect("component schemas");
        for (name, schema) in schemas {
            let text = schema.to_string();
            assert!(!text.contains("$schema"), "{name} carries $schema");
            assert!(!text.contains(r#""const""#), "{name} carries const");
            assert!(
                !text.contains(r#""type":["#),
                "{name} carries a 3.1 type array"
            );
            assert!(
                !text.contains(r#""$defs""#),
                "{name} carries $defs rather than components"
            );
            assert!(
                !text.contains(r##""$ref":"#/$defs/"##),
                "{name} references $defs instead of components"
            );
        }
        // A derived `serde_json::Value` field is `true` in 2020-12, which is not
        // a 3.0 schema object.
        assert_eq!(
            document["components"]["schemas"]["PolicyInfo"]["properties"]["resources"],
            json!({}),
        );
    }

    /// An optional request body is published as optional: `views/gc` reads an
    /// absent body as "every view", so telling a client it must send one would
    /// be wrong. The typed rows that refuse an absent body still say required.
    #[test]
    fn an_optional_body_is_published_as_optional() {
        let document = document();
        assert_eq!(
            document["paths"]["/views/gc"]["post"]["requestBody"]["required"],
            json!(false)
        );
        assert_eq!(
            document["paths"]["/acp/document/is-owner"]["post"]["requestBody"]["required"],
            json!(true)
        );
    }

    /// `deny_unknown_fields` is `additionalProperties: false`, which 3.0 allows
    /// as a boolean. Turning it into `{}` would tell a client the opposite of
    /// what the handler does.
    #[test]
    fn a_rejecting_body_keeps_additional_properties_false() {
        let document = document();
        assert_eq!(
            document["components"]["schemas"]["OwnDocumentsRequest"]["additionalProperties"],
            json!(false),
            "the handler refuses unknown fields"
        );
    }

    /// A `$ref` lifted out of an `anyOf` by the rewrite still points at the
    /// components. Reachable as soon as a field is `Option<SomeNamedStruct>`.
    #[test]
    fn a_reference_lifted_out_of_any_of_is_rewritten() {
        let mut schema = json!({
            "anyOf": [
                { "$ref": "#/$defs/Named" },
                { "type": "null" }
            ]
        });
        to_openapi_30(&mut schema);
        assert_eq!(
            schema,
            json!({ "$ref": "#/components/schemas/Named", "nullable": true })
        );
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

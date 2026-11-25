# `defra.http`: The HTTP API this node serves

- **Parents:** ["defra"]

What the HTTP surface answers beyond what Go's integration suite checks.

# `defra.http.openapi`: `GET /openapi.json` describes the routes this node serves

- **Parents:** ["defra.http"]
- **Scenarios:** ["node::openapi_document::a_node_serves_the_document_of_its_routes_and_its_alias", "handlers::openapi::tests::serves_a_document_that_describes_this_server", "handlers::openapi::tests::the_short_lived_alias_serves_the_same_document", "handlers::openapi::tests::gos_operations_survive_the_replacement", "openapi::tests::every_route_in_the_table_is_documented", "openapi::tests::security_matches_what_the_edge_actually_requires"]
`GET /openapi.json` answers an OpenAPI 3.0.3 document with content type `application/json` that describes the routes this node serves rather than copying Go's: every route the node registers appears with its method, and every operation the pinned Go baseline describes appears unchanged, schema included. An operation the node generates carries the security the edge enforces, no security at all for an exempt route, and `[{}, {bearerToken: []}]` for a route that decides for itself and runs unauthenticated when DAC and NAC are off. A route mounted at the root rather than under `/api/v0` states `servers: [{url: "/"}]` on its path, so it does not inherit the document's versioned mounts. `GET /openapi.rs.json` answers the same document.

Only a Rust node answers this document; a Go node answers with Go's own, so the scenario runs under the in-process, rust-http and cli clients and not under go-http. It fetches both endpoints from the node the scenario already owns: over a socket under rust-http and cli, and from the handler the route serves under in-process, whose node serves no socket. The unit tests above cover the rest of the document inside `defra-http`, which is where it is built.

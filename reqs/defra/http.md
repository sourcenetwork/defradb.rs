# `defra.http`: The HTTP API this node serves

- **Parents:** ["defra"]

What the HTTP surface answers beyond what Go's integration suite checks.

# `defra.http.openapi`: `GET /openapi.json` describes the routes this node serves

- **Parents:** ["defra.http"]
- **Scenarios:** ["handlers::openapi::tests::serves_a_document_that_describes_this_server", "handlers::openapi::tests::the_short_lived_alias_serves_the_same_document", "handlers::openapi::tests::gos_operations_survive_the_replacement", "openapi::tests::every_route_in_the_table_is_documented", "openapi::tests::security_matches_what_the_edge_actually_requires"]
`GET /openapi.json` answers an OpenAPI 3.0.3 document with content type `application/json` that describes the routes this node serves rather than copying Go's: every route the node registers appears with its method, and every operation the pinned Go baseline describes appears unchanged, schema included. An operation the node generates carries the security the edge enforces, no security at all for an exempt route, and `[{}, {bearerToken: []}]` for a route that decides for itself and runs unauthenticated when DAC and NAC are off. A route mounted at the root rather than under `/api/v0` states `servers: [{url: "/"}]` on its path, so it does not inherit the document's versioned mounts. `GET /openapi.rs.json` answers the same document.

Only a Rust node answers this document; a Go node answers with Go's own. The scenario that fetches the endpoints over a socket is #3282: it needs a harness-native op that fetches a path from the node a scenario already owns, because a `TestCluster` started inside the testo binary destabilises the shard. Until it lands, the five tests above prove this requirement inside `defra-http`, which is where the document is built.

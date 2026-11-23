# `defra.http`: The HTTP API this node serves

- **Parents:** ["defra"]

What the HTTP surface answers beyond what Go's integration suite checks.

# `defra.http.openapi`: `GET /openapi.json` describes the routes this node serves

- **Parents:** ["defra.http"]
`GET /openapi.json` answers an OpenAPI 3.0.3 document with content type `application/json` that describes the routes this node serves rather than copying Go's: every route the node registers appears with its method, and every operation the pinned Go baseline describes appears unchanged, schema included. An operation the node generates carries the security the edge enforces, no security at all for an exempt route, and `[{}, {bearerToken: []}]` for a route that decides for itself and runs unauthenticated when DAC and NAC are off. A route mounted at the root rather than under `/api/v0` states `servers: [{url: "/"}]` on its path, so it does not inherit the document's versioned mounts. `GET /openapi.rs.json` answers the same document.

Only a Rust node answers this document; a Go node answers with Go's own. The scenario is pending: it needs a harness-native op that fetches a path from the node a scenario already owns, since a `TestCluster` started inside the testo binary destabilises the shard (see the linked issue). `/openapi.json` is covered today by `defra_http::openapi`'s own tests.

# The network edge

Every network-facing component runs on **one tokio runtime**, held in one place: the
`Edge` (`crates/edge`). Natively it is a single runtime; in the browser it is the event
loop behind the same handle. Every network component (the HTTP server, P2P, the KMS
transport, the Orbis and Vera clients, the embeddings fetcher, telemetry export) is
handed an `Edge` at construction. Nothing finds a runtime ambiently: no
`Handle::current`, no second runtime, ever.

## What lives there

- **HTTP**: the axum server; whole data-path routes move onto the pool through an
  `OnPool` layer, document-named paths to the document's worker
  ([`crates/http/src/pool_layer.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/http/src/pool_layer.rs)); transaction routes, SSE and subscriptions stay on
  the edge (they are streams, not database work).
- **P2P**: the iroh endpoint task (all protocols muxed over one ALPN), or the libp2p
  swarm where that transport is used.
- **Remote input services**: the KMS pubsub transport, Vera clients, the embeddings
  client: single-flight, batching, byte-bounded caches.
- **pg-compat** (the Postgres wire server) and telemetry export.

## What deliberately does not

The data-path crates (storage, datastore, blockstore, crdt, defra-core, query, db, acp,
zanzibar, lens, kms, events) do not depend on tokio at all: no `tokio::spawn`, no
`tokio::time`, no `block_on`. A network task that panics fails only its own request;
every completion a worker waits on carries a deadline; request context (identity,
signing configuration, DAC bypass) travels with the task or message, never in a
thread-local.

An **on-demand** runtime (two threads) starts only when a component needs network and
was given no `Edge`: a build that starts no networking starts no runtime. The CLI
starts one `EdgeRuntime` up front and passes it down.

## Shutdown

One order, everywhere: stop accepting, drain in-flight requests with a deadline, stop
the workers, close [regolith](https://sourcenetwork.github.io/regolith/), drop the runtime outside any async context.

## Where the boundary sits

The edge is a *router*, not a worker: it terminates protocols, resolves remote inputs,
picks the target worker once, and gets out of the way. The [request
chapter](../system/request.md) traces one request across it.

> **Why one runtime, passed in.** Two failure modes the design refuses: a component
> that creates its own runtime (shutdown ordering becomes undefined, work continues
> after the server thinks it stopped) and ambient runtime capture (a library that only
> works inside a runtime, i.e. only inside someone else's lifecycle). One handle,
> injected, is the same rule as the pool: ownership explicit, lifecycle singular.

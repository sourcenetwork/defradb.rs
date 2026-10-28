# API Surfaces

One database, several fronts. Each surface is a thin shell over the same core: the
pool, the engine, the query pipeline.

## HTTP (`crates/http`)

An axum server on the edge, with data-path routes moved onto the pool:

- **GraphQL** at `POST/GET /api/v0/graphql`, subscriptions over the WebSocket
  `/graphql/ws`; Go-compatible API prefixes mounted beside the Rust ones.
- **Transaction routes** (`/tx`): begin, commit, discard, in-transaction schema and
  document work (see [Explicit transactions](../txn/explicit.md)).
- **Collections REST**: documents by DocID, describe, exists, versions, truncate;
  indexes (including Go-compatible encrypted indexes).
- **ACP**: policy add/list/get/decide, relationships; **NAC**: enable, disable, admin.
- **P2P management**: peers, connect, replicators, doc sync, relay.
- **Backup** export/import (bounded), **blocks** (verify-signature), **lens**
  (register/set/reload), **batch** (BLS batch signing), **views** (+ refresh and GC),
  **events** (SSE), **ccip** (EIP-3668 reads), health, version, debug dump.

Auth: bearer-token middleware, per-route NAC permission mapping, optional TLS.
Defaults: `127.0.0.1:9181`, 300 s timeout, bounded concurrency.

## CLI (`crates/cli`)

`defra` starts a node (HTTP, optional pg-compat, iroh or libp2p, keyring-backed keys,
optional at-rest encryption, Vera or local ACP) and carries a full HTTP client surface
(`defra client ...`: acp, backup, blocks, collections, documents, graphql, indexes,
lens, p2p, txns, views), plus keyring, identity and SDL utilities.

## Embedded (`crates/defra-node`)

One node assembly, two entry points: `NodeBuilder`, which opens a regolith store itself
(server or embedded sizing profile), and `build_with_store`, which takes a store the
application opened. Both fill in one `NodeConfig` (transport: libp2p or iroh, signing,
document ACP, node access control, query limits, sync knobs) and place the store on a
pool (`storage::start_pool`); a node with no network starts no network runtime. FFI is
built on it.

## FFI (`crates/ffi`)

A C ABI static library matching Go's `cbindings/`: node lifecycle, schema, documents,
queries and subscriptions, transactions, blocks, batch, backup, lens, indexes, ACP
(DAC and NAC), P2P. Panics are captured per call and returned as results, never
unwound across the boundary. For Go and mobile embedders.

## pg-compat (`crates/pg-compat`)

A Postgres wire-protocol server: accepts psql connections, translates SQL to the query
engine (DDL, DML, joins, aggregates, expressions, parameters), on the edge runtime.
Existing SQL tooling works against defradb.rs without a new driver.

## Around the surfaces

- **Orbis** (`crates/orbis`): a gRPC client for threshold BLS document signing by an
  Orbis ring (the M6 milestone of the KMS plan); used for BLS-augmented batch
  signatures.
- **KMS** (`crates/kms`): the key management service milestone set: a key store over
  durable IPLD blocks, an in-memory byte-bounded tier, remote single-flight batched
  key fetch over the P2P `encryption` topic, and the NAC/DAC dual gate for key
  release (document-scoped keys need DAC read; collection-scoped need NAC
  permission). Wire-compatible with Go's KMS.
- **Telemetry** (`crates/telemetry`): OTLP trace/metrics export behind a feature;
  always-on process counters (retries, conflicts by layer, storage events, engine
  tickers as one OTel counter family).

> **Why thin shells.** Every behavioural rule this book describes (ordering, ACP,
> pooling, prepared inputs) is implemented once, under the core. A surface that grew
> its own semantics would be a second database with worse tests; the HTTP layer, the
> CLI, the FFI, the SQL bridge and two embedders all deliberately share one.

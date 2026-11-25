# Transports and the peer

defradb.rs speaks one P2P transport, [pinc](https://github.com/sourcenetwork/pinc), in
its own crate (`crates/database/pinc`). The sync engine it drives lives in
`crates/database/p2p`.

- **Direct connections (QUIC over UDP)**: every Defra protocol, embedder stream
  protocols included, is a tagged stream on one connection per peer. One endpoint task
  owns all pinc state and is commanded over a bounded channel
  ([`crates/database/pinc/src/endpoint.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/pinc/src/endpoint.rs));
  it runs on one edge thread over a nuclei UDP socket.
- **Topics through hubs**: gossip head hints and the KMS key exchange are published on
  topics that hubs carry to every node that joined them
  ([`crates/database/pinc/src/topics.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/pinc/src/topics.rs)).
  A node names its hubs (`net.pinc_hubs`); `defradb hub` runs one. A node that names
  none syncs only over direct requests.
- **Peer exchange**: a node hands each new peer the peers it is connected to, so peers of
  a peer connect ([`crates/database/pinc/src/exchange.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/pinc/src/exchange.rs)).

The adapter surface (`crates/surfaces/p2p-adapter`) wraps the peer for the CLI, the
embedded node and the FFI. All P2P runs on the edge runtime
([`crates/entry-points/defra-node/src/pinc_runtime.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/entry-points/defra-node/src/pinc_runtime.rs)).
The browser client runs no P2P.

## Peer identity

A node has one peer key (Ed25519, from the keyring: name `peer-key`); its seed is the
pinc identity, and a peer is addressed as `<peer id>@<udp address>`. The QUIC layer
authenticates the connection; on top of it, an optional Defra identity challenge binds
the peer's Defra DID to the connection.

## The read gate

Replication carries data across a trust boundary, and gated access applies the same
ACP read rules as a local query before a block is served
([`crates/surfaces/p2p-adapter/src/read_gate.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/surfaces/p2p-adapter/src/read_gate.rs), [`crates/database/db/src/block/verify.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/db/src/block/verify.rs)). A
replicator registered for a collection without filters receives it; a filtered
replicator's access is checked against its filters. A peer with a connection is not a
peer with permission where the gate applies: document-level ACP remains the
authoritative boundary, and replicated blocks are checked (CID integrity, attached
signatures) before merge.

> **Why one endpoint task.** A shared socket must have one owner. Endpoint state
> scattered across tasks (each subsystem dialing its own connections) produces
> connection sprawl, races on migration, and no place to enforce admission. One task,
> one channel, one admission policy: the same ownership rule as the worker pool, at
> the network boundary.

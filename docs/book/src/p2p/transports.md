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
  none syncs only over direct requests. A topic message is at most 64 KiB: a larger
  publish fails with `GossipTooLarge` before anything is sent, and the KMS splits its
  key requests so that each request and its reply fit (470 keys a request).
- **Relay through a hub**: `defradb hub --relay` (off by default) also relays sessions
  between its members, so two nodes behind NATs reach each other by peer id alone,
  without knowing each other's address, until a direct path works. It relays over UDP
  only: a node whose UDP is blocked is not reached.
- **Peer exchange**: a node hands each new peer the peers it is connected to, so peers of
  a peer connect ([`crates/database/pinc/src/exchange.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/pinc/src/exchange.rs)).

The adapter surface (`crates/surfaces/p2p-adapter`) wraps the peer for the CLI, the
embedded node and the FFI. All P2P runs on the edge runtime
([`crates/entry-points/defra-node/src/pinc_runtime.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/entry-points/defra-node/src/pinc_runtime.rs)).
The browser client runs no P2P.

## Binding and addresses

`defradb start` binds `net.pinc_bind_addr`, `127.0.0.1` when unset, on
`net.pinc_bind_port` (a port the OS picks when unset). Bound where other hosts can
reach it with an empty `net.pinc_allowed_peers`, it logs a warning: any peer may
connect. An embedder (`defra-node`, the FFI) that names no bind binds `127.0.0.1` on a
port the OS picks too; it names `0.0.0.0:<port>` to listen on every interface.

A node bound on a wildcard address lists one address per interface, never loopback or
IPv6 link-local. The addresses in `net.pinc_public_addrs` (`--pinc-public-addrs`; for a
hub, `--public-addr`), such as a public address in front of a NAT, are listed first.
A hub on a wildcard bind with no address to list fails to start.

A block sync (a CAR fetch) waits `block_sync_timeout` (30 s by default; the FFI's
`p2p_block_sync_timeout_ms`) for a provider's blocks.

An embedder stream protocol's handler is dropped at its deadline and its stream reset
with code 7, even when its last write is blocked on an opener that stopped reading.
An open refused as busy writes nothing, so a retried open runs its handler once.

## Peer identity

A node has one peer key (Ed25519, from the keyring: name `peer-key`); its seed is the
pinc identity, and a peer is addressed as `<peer id>@<udp address>`. The QUIC layer
authenticates the connection; on top of it, an optional Defra identity challenge binds
the peer's Defra DID to the connection. An FFI node keeps its peer key in its store, so
a node reopened on the same store keeps its peer id.

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

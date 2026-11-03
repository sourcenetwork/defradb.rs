# Transports and the peer

defradb.rs speaks two P2P stacks, feature-gated in one crate (`crates/p2p`):

- **iroh (QUIC)**: the default for `defra-node`, the CLI's `defra start`, the embedded
  and FFI nodes, and the browser. One background task owns all iroh state and is
  commanded over a channel ([`crates/p2p/src/iroh/endpoint.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/p2p/src/iroh/endpoint.rs)); every Defra protocol, including
  gossip, is multiplexed over a single ALPN (with a v1 fallback), so there is one
  connection per peer, not one per subsystem. An optional self-hosted relay server
  exists for NAT traversal.
- **libp2p**: a full swarm (gossipsub, kad, identify, relay, QUIC) used where the
  libp2p ecosystem is the deployment requirement. The standalone `crates/bitswap`
  (wantlists, ledgers, peer task queue) serves block fetches on this path.

Both stacks sit behind the same adapter surface, so the sync mechanisms, the read gate
and the security model are shared. All P2P runs on the edge runtime
([`crates/defra-node/src/p2p_runtime.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/defra-node/src/p2p_runtime.rs)).

## Peer identity

A node has one peer key (Ed25519, from the keyring: name `peer-key`), shared across
transports; a node that predates the shared key keeps its existing identity. On iroh,
the QUIC layer already authenticates the connection; on top of it, an optional Defra
identity challenge binds the peer's Defra DID to the connection
([`crates/p2p/src/peer_identity.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/p2p/src/peer_identity.rs)).

## The read gate

Replication carries data across a trust boundary, and gated access applies the same
ACP read rules as a local query before a block is served
([`crates/p2p/src/bitswap/read_gate.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/p2p/src/bitswap/read_gate.rs), [`crates/db/src/block/verify.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/block/verify.rs)). Two
deployment choices deliberately bypass the gate: an open Bitswap access mode serves
without ACP checks, and a replicator registered for a collection without filters
receives it; a filtered replicator's access is checked against its filters. A peer
with a connection is not a peer with permission where the gate applies:
document-level ACP remains the authoritative boundary, and replicated blocks are
checked (CID integrity, attached signatures) before merge.

> **Why one endpoint task.** A shared socket must have one owner. Endpoint state
> scattered across tasks (each subsystem dialing its own connections) produces
> connection sprawl, races on migration, and no place to enforce admission. One task,
> one channel, one admission policy: the same ownership rule as the worker pool, at
> the network boundary.

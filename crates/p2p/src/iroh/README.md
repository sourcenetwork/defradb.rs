# Iroh Transport Notes

DefraDB's Iroh transport uses the same `P2PTransport` surface as libp2p, but the
wire mechanics are intentionally different where Iroh has different primitives.

## One connection per peer

Every Defra protocol shares a single QUIC connection, negotiated on the mux
ALPN `/defra-iroh/mux/0.1`. A stream names its protocol in-band: the first
frame it writes is a one-byte length followed by a tag such as
`/defra-iroh/docsync/0.1`, and `endpoint_streams::dispatch_stream` routes on
that tag. `protocols.rs` holds the ALPN, the tags, and the frame helpers.

The discriminator has to be in-band because ALPN is negotiated once per TLS
handshake. Naming each protocol with its own ALPN — as this transport did until
the tags were introduced — costs a separate connection, congestion controller,
path MTU probe, hole-punch, and keepalive timer per protocol against the same
peer. An embedded client touching identity, replication, doc sync, branchable
sync, and CAR fetch paid five of each.

Two consequences worth knowing:

- The outbound connection cache in `endpoint_rpc.rs` is keyed by `EndpointId`
  alone, and an explicit dial adopts its connection into that cache. A stream
  failure therefore must not evict it — `evict_if_closed` drops a cached entry
  only once QUIC itself reports the connection closed, since every other
  protocol is still using it.
- `iroh-gossip` keeps its own ALPN. It owns that handshake and takes whole
  connections, so a gossiping node holds two connections to a peer, not one.

DocSync and BranchableSync ride these streams rather than the `pubsub_rpc` layer
introduced for libp2p in #828. The `pubsub_rpc` topic model is tied to libp2p
peer IDs and gossipsub topic meshes. Iroh peers are addressed by `EndpointId`,
so reusing the libp2p response-topic convention would require a peer-id
translation layer that does not exist on the Go-compatible wire path.

The CBOR message structs stay shared with libp2p; only the transport envelope is
Iroh-specific. Iroh SE push and SE query messages are signed with the sending
transport identity and are verified against the QUIC peer that opened the stream
before transport events are emitted.

## Libp2p-only transport methods

These `P2PTransport` methods are libp2p-only by design:

- `publish_raw`
- `subscribe_raw`
- `register_pubsub_rpc_topic`

They exist for libp2p's `pubsub_rpc` dispatcher and are left as the default
not-supported/no-op behavior on Iroh.

`topic_peers()` is also not a full libp2p equivalent on Iroh. libp2p can ask
gossipsub for all peers known for a topic. `iroh-gossip` exposes the direct
neighbors of a joined topic, so Iroh returns those topic-scoped neighbors rather
than all connected peers. This keeps the result topic-specific, but callers must
not treat it as complete topic membership. If the local node has not subscribed
to the topic, Iroh returns an empty list; callers that relied on the previous
all-connected fallback should subscribe first or use `connected_peers()`.

## Audit follow-up references

This file documents the Iroh transport decisions behind #965, #966, #967, and
#968, and the surrounding P2P audit work tracked by #962 through #968.

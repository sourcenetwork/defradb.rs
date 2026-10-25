# Providers, admission, budgets

## Provider discovery: signed service records

How do peers find who has what? Nodes publish **service records**: a signed, expiring
statement of what this node offers (collections with roles Replica/Archive/Reader,
replicator service, searchable-encryption query service, KMS, relay, custom roles),
published via pkarr, served on query streams, and announced on a discovery topic
(`crates/p2p/src/iroh/records/`). Readers keep the newest valid record per node in a
book and pick fetch candidates and gossip neighbours from it. Record freshness,
fan-in of replies, and the alternate-slot rule for the best provider are modelled:
the provider record family has TLA+ and Lean proofs (see
[Verification](../proofs/index.md)).

## Admission: who may connect

- **Allowlists** ([`crates/p2p/src/iroh/endpoint_config.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/p2p/src/iroh/endpoint_config.rs)): `AcceptAll` (default) or an explicit
  endpoint set, mutable at runtime. Removing from the allowlist stops *new* admissions;
  **`PeerAdmission::revoke`** is the operation that actually bars a peer, and the
  admission authority makes revoke -> re-admit a lockstep state machine so a revoked
  peer cannot slip back in through a stale allowlist update.
- **Peer identity challenges** (optional): bind the Defra identity to the connection.
- **Inbound budgets** ([`crates/p2p/src/iroh/inbound_budget.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/p2p/src/iroh/inbound_budget.rs)): concurrent inbound streams are
  bounded; excess is refused, not queued unboundedly.

## Trust and capability on the sync path

Replicator registration is outbound intent, not inbound permission: accepting PushLog
data requires membership/subscription and explicit-replay authorization for the
content; gated serving crosses the read gate (ACP) and block checks, while an open
access mode or an unfiltered registered replicator is served without them. A
separate capability regime governs *explicit replay* (re-broadcast of previously seen
pushes), with its own revocation, modelled in [`crates/p2p/src/explicit_replay.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/p2p/src/explicit_replay.rs).

## Budgets, end to end

Backpressure is a property of the whole path, not a per-queue setting: bounded push
queues (items and bytes), per-peer rate limits, bounded concurrent fetches, CAR caps
(10,000 blocks / 16 MiB), doc-sync request caps (1000 IDs), pooled and persisted
pending-DAG storage, and the retry sweep on a fixed interval. Overload at the network
edge waits on a semaphore; overload inside the node surfaces as queue depth, never as
unbounded memory.

> **Why signed, expiring records instead of a DHT of availability.** A peer's claim
> "I hold collection X" is useless if stale: the fetch fan-out burns a round trip per
> dead claim. Expiry forces re-assertion; signatures make records unforgeable
> (availability claims become attributable); and the book of newest-valid-records is
> small, local and cheap to consult, where a global availability index would be a
> coordination point the design does not otherwise need.

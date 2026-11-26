# Providers, admission, budgets

## Provider discovery: peer records and peer exchange

How do peers find each other? Each node signs a record of the addresses it can be
dialed at and hands it to every peer it connects to, so a peer can later dial it by id;
a connected node also hands each new peer the peers it holds (peer exchange,
[`crates/database/pinc/src/exchange.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/pinc/src/exchange.rs)).
Fetch candidates for a DAG come from the peers that announced its heads and from the
provider ranking in the sync engine.

## Admission: who may connect

- **Allowlists** ([`crates/database/pinc/src/admission.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/pinc/src/admission.rs)): `AcceptAll` (default) or an explicit
  peer set, mutable at runtime. Removing from the allowlist stops *new* admissions;
  **`PeerAdmission::revoke`** is the operation that actually bars a peer, and the
  admission authority makes revoke -> re-admit a lockstep state machine so a revoked
  peer cannot slip back in through a stale allowlist update.
- **Peer identity challenges** (optional): bind the Defra identity to the connection.
- **Inbound budgets** ([`crates/database/pinc/src/inbound_budget.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/pinc/src/inbound_budget.rs)): concurrent inbound streams are
  bounded; excess is refused, not queued unboundedly.

## Trust and capability on the sync path

Replicator registration is outbound intent, not inbound permission: accepting PushLog
data requires membership/subscription and explicit-replay authorization for the
content; gated serving crosses the read gate (ACP) and block checks, while an open
access mode or an unfiltered registered replicator is served without them. A
separate capability regime governs *explicit replay* (re-broadcast of previously seen
pushes), with its own revocation, modelled in [`crates/database/p2p/src/explicit_replay.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/p2p/src/explicit_replay.rs).

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

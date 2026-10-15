# Transport — the link under the head hint

Family: `p2p`. First slice of the transport axis; no Lean companion.

## Why this model exists

Every other model in this corpus abstracts delivery away. `Convergence.tla`
carries a `connected \subseteq NodePairs` edge set and `HasConnectedProvider(n, b)`,
and a block moves from holder to wanter in one atomic action. `SyncOwnership.tla`
goes further and drops the peer set entirely: all other nodes collapse into
`ProviderMode`, a knob naming the least-qualified peer admitted to a fetch
rotation, with transport facts (`OriginUnroutable`, `RelayOnly`,
`OriginAuthMode = "TransportBound"`) appearing as adjectives on that knob.

Both abstractions are deliberate and both are sound for what they prove. Neither
can express a message that is *in transit*. This model makes the wire explicit
for exactly one exchange — a PushLog head hint and its reply — because two
properties of the real transport are invisible at the old level of abstraction
and both are load-bearing.

**A send that returns `Ok` has not been delivered.** Every method on
`P2PTransport` (`crates/p2p/src/transport.rs:286`) is
`async fn send_*(...) -> Result<()>`; the reply arrives later as a separate
inbound `TransportEvent`. `handle_send_two_stream_request`
(`crates/p2p/src/host/command_handler/messaging.rs:65`) waits 30 seconds
(`:83`), then calls `cleanup_pending` and returns
`Err(Transport("timeout waiting for response"))` (`:97-99`). That error does not
distinguish a request that never arrived from one that was delivered, processed
and whose reply was lost.

**On libp2p, the reply is a second, independent delivery.** Go compatibility
fixes that shape (`crates/p2p/src/two_stream/mod.rs:1-8`): the sender opens
`/defradb/rep_req/0.0.1`, sends, and **closes** the stream; the receiver opens a
**new** stream on `/defradb/rep_resp/0.0.1` to answer. A reply therefore needs a
working route from receiver back to sender, which a relay-only path does not
provide. It cannot be changed: Go verifies by re-serializing, so a capability
field it does not know would break signature verification.

**On iroh, it is not.** The reply rides the request's own QUIC bidirectional
stream, through the response token
(`crates/p2p/src/iroh/transport.rs prefers_same_stream_reply`). `ReplyMode` is
therefore a per-transport constant, not a per-peer negotiation, and the model
runs both settings.

## State and actions

One directed sender→receiver exchange for one scope. `localHead` abstracts the
current composite head CID as a monotone version, matching `SyncOwnership`'s
`localV`.

- `marker` is the sender's durable obligation: a presence marker, not a payload.
  A retry rederives `localHead` rather than replaying a stored CID — the
  `MarkerRederive` design `SyncOwnership` models at the node level.
- `awaiting` is the single live attempt. It is the sender's `SingleFlight`.
- `inflight[h]` is the number of request copies still on the wire. `Send` puts
  two copies on a `Duplicate` link and one on a `Dedup` link, modelling iroh
  multipath delivering a direct and a relayed copy of the same hint
  (`crates/p2p/src/iroh/transport.rs:971`, whose comment notes that a direct
  link "would deliver the unrelayed copy first and dedup would hide the relay").
- `replyReady` is the return wire. `SendReply` is gated on `ReplyRoutable`,
  which is false exactly when a `NewStream` reply meets a `RelayOnly` path.
- `registered` / `merged` are the receiver's durable and merged heads;
  `obligations[h]` counts the units of work one head hint created.

`Timeout` does **not** recall the copies already on the wire. A timed-out
request may still be delivered, which is the whole point.

## Runs

| Config | Knob changed | Verdict |
|---|---|---|
| `MC_Transport_Green` | hostile link, shipped design | GREEN, incl. both liveness properties |
| `MC_Transport_Green_OrderedLastArrived` | `LinkMode = "Ordered"` | GREEN |
| `MC_Transport_Red_DefiniteTimeout` | `TimeoutMode = "Definite"` | RED `INV_NoLostUpdate` |
| `MC_Transport_Red_ReorderOverwrite` | `RegisterMode = "LastArrived"` | RED `INV_RegisteredMonotone` |
| `MC_Transport_Red_DuplicateObligation` | `+ DupMode = "Duplicate"` | RED `INV_ObligationIdempotent` |
| `MC_Transport_Green_SameStream` | `ReplyMode = "SameStream"` + `RelayOnly` | GREEN — the iroh shape |
| `MC_Transport_Red_RelayNoRouteBack` | `RouteMode = "RelayOnly"` | RED `LIVE_SenderQuiesces` — the libp2p shape |

The green run is the hostile link against the shipped design: the wire reorders,
duplicates, requires a reverse route for every reply, and times out ambiguously,
while the sender rederives from a marker and the receiver registers head-current.
That combination holds every invariant and both liveness properties.

`MC_Transport_Green_OrderedLastArrived` exists to place the blame correctly.
Arrival-order registration is not wrong in itself; it is wrong against a link
that reorders. The config differs from `MC_Transport_Red_ReorderOverwrite` in
`LinkMode` alone.

## Findings

### A timeout retires the wrong head's obligation

`MC_Transport_Red_DefiniteTimeout` does not produce the trace this model was
written to catch. The expected counterexample was a lost request whose timeout
clears the marker. The trace TLC finds is shorter and worse:

1. `Write` — `localHead = 1`, marker set
2. `Send` — head 1 on the wire, `awaiting = 1`
3. `Write` — `localHead = 2`, **while head 1's attempt is still live**
4. `Timeout` on head 1 — `Definite` clears the marker

Head 2 is now lost having never reached the wire. No packet was dropped.

The distinction this draws is not "ambiguous vs definite timeout". It is that
**marker retirement needs the head-current guard on every path that retires it.**
`AckReply` already has it (`awaiting = localHead`); the timeout path does not.
This is the same guard `PushCoalescing.tla` models as
`RetryMode = "CurrentOnly"`, on a different edge.

It follows that a `Definite` timeout carrying the head-current guard might also
be safe — this model does not decide that, because it has no such mode. That is
the first red to add next, and until it exists the model proves the guard is
*sufficient* on the ambiguous path, not that ambiguity is *necessary*.

### Single-flight attempts do not imply one head on the wire

`Send` admits one attempt at a time, yet `MC_Transport_Red_ReorderOverwrite`
reaches a state with two heads in flight: a timed-out attempt's copies are never
recalled, so a retry of a newer head races them. Any receiver-side reasoning
that assumes the sender's single-flight discipline bounds what can arrive is
unsound.

### Two-stream is what couples reply delivery to reverse routability

`MC_Transport_Red_RelayNoRouteBack` violates `LIVE_SenderQuiesces` while every
safety invariant holds and the head merges. The receiver has the data; the
sender simply never learns, and retries forever. `MC_Transport_Green_SameStream`
differs in `ReplyMode` alone and quiesces. The unbounded retry is therefore a
cost of the Go-compatible reverse-stream shape, not of the relay.

That pair maps onto the two transports rather than onto two hypotheses: the RED
run is libp2p, which cannot escape the shape without breaking Go's signature
check, and the GREEN run is iroh, which answers on the request's own stream. The
model describes the fix that was chosen, and prices what it buys.

## Boundaries

- **One hop, one direction, one scope.** Multi-hop composition is not modelled
  here. `SyncOwnership.tla:49-53` claims that composition in a comment; that
  claim remains unchecked.
- **Bounded faults.** `MaxLoss = 1` and `MaxHead = 2`. The liveness properties
  are checked under a bounded number of link faults; an adversary permitted
  unbounded loss prevents progress trivially and is not an interesting
  counterexample.
- **Timeouts are not spurious.** `Timeout` is enabled only when the reply
  genuinely cannot arrive (the request has not landed, or no reverse route
  exists). A real 30s timeout can also fire while a successful attempt is still
  in progress. Admitting that would make every liveness property here
  unprovable without a retry budget, so it is excluded from this first pass and
  is the second red to add.
- **One reply shape per run.** `ReplyMode` is fixed per configuration, matching
  a transport. A node that speaks both transports at once runs both shapes
  side by side; nothing here relates them.
- **The gossip path is not modelled.** `broadcast_update`
  (`crates/p2p/src/sync/broadcaster.rs:77`) publishes to a document topic and a
  collection topic and tolerates either failing
  (`BroadcastResult::PartialDocumentOnly`, `:106`) with a warning. That is a
  second delivery class with weaker guarantees and it needs its own slice.
- **No head-of-line blocking.** All inbound events share one bounded scheduler
  with four classes (`crates/p2p/src/sync/event_dispatcher.rs:27`:
  `Inline`, `Admission`, `Recovery`, `Completion`). Cross-class starvation over
  a link is not represented; `SyncOwnership`'s `SharedServeWorkers` and
  `WorkerSaturated` modes cover the node-local half only.

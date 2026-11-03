# SyncOwnership — head-hint ownership transfer

This model carries the approved sender-demotion stage of #1116 into the current
proof layout. A sender durably records a document or collection scope, announces
one current composite/collection head, and clears its marker only after an
acknowledgement for that same current head. A receiver either reports the head
already merged or durably registers a bounded missing-DAG obligation before
acknowledging it. The receiver's #1123 per-root clock is the sole fetch-claim
boundary; CAR completion retires the durable obligation.

Companion models retain their narrower contracts:

- `PushLogAdmission.tla`: overflow nacks instead of laundering a dropped
  registration into success.
- `PendingDagRestart.tla`: a success-acked registration survives restart.
- `PushBacklog.tla`: resident outbound work and task handles remain bounded.
- `PushCoalescing.tla`: superseded live heads do not recreate stale work. Stage
  3 replaces its CID-valued durable ledger with the head-current marker guard
  modeled here.

`MC_SyncOwnership_Dispatch.tla` extends this model with the receiver clock's
slot dispatch, retry deadlines and the pending TTL; see *Receiver dispatch*
below. `MC_SyncOwnership_Memory.tla` extends it with the bounded in-memory
pending map; see *In-memory pending map* below.

## State and actions

`Docs` and `Cols` are both replayable scopes. `localV` and `mergedV` abstract
current composite or collection head CIDs with monotone versions.

- `dirty` is the permitted sender durability: a set of scopes only.
- `payloadLedger` is the current-main countermodel: sender durability contains
  a scope plus a particular CID/version.
- `inflight` contains idempotent head hints.
- `pending` is the receiver's bounded durable want queue (`Cap`). `FetchCap`
  separately bounds roots that have crossed the clock's fetch-ownership seam;
  durable wants may wait, but scheduled event handoffs and retained tasks may
  not form another queue behind the attempt semaphore.
- `originBound` records the qualified recovery origin chosen for each durable
  root generation. A same-root replay is idempotent and cannot replace that
  binding without linked-DAG availability evidence. `OriginRebound` retains
  the counterexample where a relay replay captures the provider slot.
  `ProviderOf(root)` makes the provider identity dimension explicit: an
  `Origin` binding is serviceable, while the red action changes the same live
  root to a `Relay` binding without changing its CID or scope.
- `qualifiedProvider` records that the root has at least one routable peer that
  owns the linked DAG. Usually the bound origin establishes both facts. In an
  Iroh relay chain, the signed envelope can authenticate an origin that the
  receiver cannot route to; a later direct same-root hint from a peer that has
  completed the DAG adds a durable alternate without replacing `originBound`
  or resetting the root's retry clock. The alternate set is bounded in Rust
  and survives receiver restart with the pending obligation.
- `qualifiedOffers` separates delivery of that authenticated direct hint from
  its durable admission. In the relay-chain configuration, fairness applies to
  B's existing downstream scope marker after B has completed and merged its
  own upstream receiver instance. An ordinary gossip relay or exact-root-only
  relay cannot create this offer; Rust requires authenticated direct PushLog
  delivery, including the configured or signed two-stream replicator seams.
- `serveAuth` records the volatile exact-root CAR grant installed before a
  hint. It is always lost on sender restart.
- `serveScopes` records the exact-root block-serving policy from which authority
  can be reconstructed. In the green policy the authenticated CAR requester,
  the requested root, and either durable replication configuration or ACP read
  authorization for the root reconstruct that capability after restart. This
  matches Go's Bitswap access boundary and avoids making durable completion
  depend on the ordering of a process-local gossip-neighbor observation. A
  fetch may use either the live grant or derive the rooted capability from this
  policy.
- `flights[root]` counts CAR fetch owners for an exact root.
- `drained` records whether a fetch owner retained its productive CAR stream
  through transport completion instead of cancelling after the first block.
- `lostCompletions` records a transport terminal result that the fetch owner
  cannot observe. The green `CompletionMode` latches early results, always
  drains state-bearing events, and services host response commands while event
  delivery is backpressured. Red modes discard an edge before waiter
  registration, stop draining behind saturated spawned workers, or block the
  host on a full event channel while an admitted handler waits for that host's
  response command. Each strands the sole fetch owner.
- `terminalFlights[root]` counts durable pending-record cleanup writers.
  Production admits one; the duplicate-terminal red policy permits two
  merge/quarantine observations to contend on the same storage key.
- `ProviderMode` distinguishes origin-bound recovery, Iroh recovery through a
  later qualified alternate, and authenticated-hop recovery. It abstracts the
  least-qualified peer admitted to the provider rotation. Iroh may use the
  independently verified publisher only when that
  endpoint has a reconnectable transport route. A temporary disconnect is a
  stuttering interval, not a terminal disposition: the durable root stays
  pending and the one receiver clock waits for fair reconnection. Alternates
  require either positive evidence for a CID on the missing frontier or
  authenticated direct PushLog delivery from a sender path that owns rooted
  CAR authority. Connectivity or root possession observed through a relayed
  gossip envelope is insufficient. An authenticated gossip hop is a red mode:
  the fleet demonstrated that a
  relay may possess the announced root while all linked descendants are absent.
  The other red modes retain an unroutable origin or an unverified relay.
- `OriginAuthMode` distinguishes an origin bound by native transport metadata
  or an endpoint-key signature from an unsigned peer ID copied out of the
  gossip payload. A signed CRDT block proves content authorship, not that the
  named endpoint currently holds and can serve the linked DAG.
- `FetchMode` distinguishes bounded rooted discovery followed by a capped CAR
  descendant closure from the already-known missing frontier from both
  extremes exposed by fleet evidence: an unbounded recursive historical-root
  walk, and exact-CID requests that have no routable provider after sender
  demotion removes per-CID advertisements. This is the Rust CAR analogue of
  Go's one per-root blockservice session: one receiver owner advances through
  linked blocks without opening a new transport round trip for every DAG
  layer. The existing CAR block/byte caps bound each response; a truncated
  response recomputes the frontier through the same owner and pacing clock.

`Update` records sender durability before exposing the live hint. `ReHintMarker`
has no stored version to replay and therefore must read `localV[scope]`.

`MarkerHandoff` splits `Update` for a transactional write, whose scope markers
are registered after its commit by a coordinator task handed off in
commit-receipt order (`merge/txn_broadcaster.rs`
`SyncTxnBroadcaster::submit_update`, called from `event/emission.rs`
`register_update_event_callback`). The commit call waits for that task and
returns when it ends or is dropped unrun; only then is the writer told the
write committed.

- `handoff` holds the scopes committed but not yet registered: their
  `ordered_transaction_broadcast` tasks, each holding the `BroadcastDone` its
  commit call waits on. It is volatile.
- `senderUp` is false once the sender's P2P stopped or its process died
  (`SyncCoordinator::spawn_background_task` no longer accepts tasks).
- `Commit` makes the new head current; `RegisterMarker` records the marker,
  sends the hint and returns the commit call.
- `SenderStop` is P2P's graceful shutdown. Under `Drained`, EmbeddedNode::shutdown
  drains the requests it admitted before P2P stops (`NodeShutdown.tla`
  `INV_AdmittedWritesMarked`), so nothing is pending then. Under `Undrained`
  P2P stops with registrations pending, refuses or aborts them, and each
  dropped task returns its commit call.
- `SenderCrash` is a kill between a commit and its registration: the
  registration is lost, and no writer was told the write committed.

`Atomic` keeps the one-step `Update` for every other configuration: without a
sender stop or crash between them, no receiver action can observe the step
between a commit and its registration.
`ProcessMerged` and `ProcessRegister` are the only success-ack actions.
`ProcessNack` leaves sender ownership intact. Every failed delivery, including
an actionable capacity nack, advances the one durable per-peer retry clock on
the Go-compatible 30s, 1m, 2m, 4m, 8m, 16m, 32m ladder. The 2-second sweep is
only a due-record poll; it is not an alternate retry cadence. Rust implements
that state machine once in `defra-p2p-adapter::retry`: CLI, embedded, and
defra-node, over both libp2p and Iroh, construct the same failure recorder and
marker-rederive loop. A disconnected peer is redialed only after its durable
schedule is due, and that failed delivery attempt advances the existing rung;
a later reconnect makes the same rung immediately due without resetting it.
The transport-specific document pusher and replay implementations have likewise
collapsed into one `P2PTransport` path, so no runtime can select different
marker, ack-fence, rederive, or pacing semantics.

`ClaimFetch` represents every
retry-expiry, connect-expedite, and partial-progress trigger after coalescing
through the same #1123 clock claim. A tick with no currently connected
qualified provider does not claim a fetch owner and does not exhaust the root;
it is a stuttering refinement until reconnect expedites that same clock.
The Rust conformance seam follows this literally: live registration, partial
CAR progress, reconnect, and restart may create or expedite a due root, but
only the receiver clock emits `DagNeedsFetch`. The clock reserves one of the
bounded root-owner slots before advancing a retry rung; the reservation covers
both the event handoff and retained fetch task, so pending roots do not form a
hidden task queue behind the attempt semaphore. In particular, registration
does not retain a transport reply or the pending-state writer while waiting on
the merge/fetch event channel. `CompleteFetch` does not remove `pending`:
single and batched Rust merge paths retain the same live registration until
merge/mark or quarantine reaches a durable terminal outcome. If merge returns
a transient error, the existing per-root clock sees the locally complete root
and re-emits merge work on its paced rung; restart and peer-connect resync are
not alternate cleanup/retry owners.
`Crash` clears in-memory fetch owners and, only in the volatile red policies,
receiver registrations or CAR-serving authority. In particular, a
collection-topic PushLog does not require an outbound replicator record or a
previously observed subscriber entry on the source: the authenticated CAR
requester and the exact requested root are checked against the source's durable
replicator/ACP policy to reconstruct the serving scope.

`CompleteTerminal` is also the cancellation point for a fetch claimed during
the merge-to-durable-cleanup window. Production represents this with the
pending generation lease: removing or superseding the registered root makes
the retained fetch owner non-current, so it cannot occupy `FetchCap` after the
obligation is terminal.

`ClaimMerge` distinguishes batching from concurrency: one receiver merge
writer may claim several ready roots and commit them together, but two
independent writers may not overlap. This matches Go's explicit P2P merge
serialization while retaining Rust's ordered batch transaction optimization.
Frontend and transport adapters are not allowed to select a different merge
ownership policy.

`SameRootReannounce` makes provider retention explicit rather than deriving it
from a process-global provider assumption. The green transition preserves
`originBound`; the red rebind configuration loses it while the receiver still
owns `pending`. `OfferQualifiedAlternate` and `AddQualifiedAlternate` model the
Iroh chain case separately: the immutable origin remains bound, the
independently authenticated direct announcer becomes serviceable only after it
owns the complete DAG, and `ClaimFetch` cannot run until that qualified
provider is durable.

`ProcessRegister` acks a root already pending as it is. Any other head
registers on its own, and retires the scope's current head (`ScopeHead`, the
newest registered) only when it links it: each write links the head before
it, so `<<s, v - 1>>` is the one parent. Nothing is acked on another head's
strength, so a stale lower version that arrives after its child takes its own
registration until the child merges. Both green configurations use
`MaxV = 2`, so supersession, late arrival of the stale lower version, and
stale-ack protection are reachable rather than vacuously excluded by the model
constants.

The two full green configurations check the cross-scope safety product. The
fairness checks are split into document, collection, and Iroh relay-chain
configurations. The document and collection configurations each remain at
`MaxV = 2`; this keeps the temporal graph tractable while exercising
supersession and eventual quiescence for both replayable scope kinds.

For a configured multi-hop replicator chain, the model composes once per hop.
After B completes `CompleteMerge` for A's head, B may become the sender of a
new downstream instance to C: B records its B→C scope marker and announces the
same current head only after B owns the complete DAG. This matches Go's
post-merge `Update` → `SendUpdate` replicator fanout. Rust performs that narrow
explicit-replicator fanout independently of optional gossip rebroadcast, so
stage 3 does not enable the stage-4 gossip-direction change. A root-only gossip
relay has not completed `Merge` and therefore cannot become the downstream
provider.

The hand-off between the two hops is modelled, not assumed. The receiver is
the hub B: `fwdDirty` is B's durable B→C scope marker, `downV` is the head C
holds, and `DeliverDownstream` abstracts C's instance of this same protocol
(the one-hop machine is what delivers a durable marker). B's upstream record
in `pending` and B's downstream marker must overlap: `ClaimTerminal` records
the marker before `CompleteTerminal` discharges the record. In Rust,
`replication/handlers.rs` `forward_merged` registers B's replicator markers
before `mark_as_merged` discharges the pending record. The terminal step's
guard is what the handler checks durably (`forward_owed`): the database holds
the root, because it merged it or answers a terminal skip that
`MergeOutcome::skip_reason_is_applied` accepts, and the root's merged bit is
unset. The bit is set only after a durable forward, so every route to the
terminal step forwards first: a fresh merge, and an "already merged" re-drive
in the same process or after a restart (the database answers from its durable
head publication, not by merging again). The re-driven-merge sink in
`db/src/merge/redriven_sink.rs` keeps the same order, and
`merge_handler/dispatch.rs` `redrive_deferred` hands it an applied skip whose
merged bit is unset as well as a merge. A forward that fails is
the terminal step not having happened: the root stays unmarked, its record
stays, and the receiver clock re-drives it.

`MergeMode = "DischargeBeforeForward"` is the order this replaced:
`mark_as_merged` first, then the forward from the still-running handler
(`fwdDue`, volatile). A crash between the two (`Crash`), a forward that fails
after the discharge (`ForwardFail`: shutdown, an unreadable replicator list,
or a recorder that cannot make the marker durable), or a re-drive that finds
the root already merged after a crash, each leaves a head C lacks with nothing
on B that still owes it.

`MergeMode = "OwedInMemory"` keeps the forward before the mark but decides it
from process memory: the terminal step forwards only a root the running
process merged or failed to forward (`fwdDue`). A crash between the merge
commit and the terminal step empties that memory, the re-drive gets the
database's durable "already merged", and the root is discharged unforwarded.

Reconnect is evidence that an existing sender scope marker is deliverable
again, so the runtime moves that peer's existing marker schedule to "due now"
without resetting its retry rung. This is an enabling event for
`ReHintMarker`, not a second sender retry loop or a second receiver fetch
trigger. On libp2p, the claimed receiver owner waits for the rooted CAR response
event (while retaining its bounded timeout and lease checks); transport
completion therefore releases the one-root admission slot without a polling
delay.

## Receiver dispatch

`MC_SyncOwnership_Dispatch.tla` runs every action of this model and adds the
clock's queue around `ClaimFetch`. After a hub restart the clock can hand
every fetch slot to restored catch-up roots whose providers stall. If each
fetch kept its slot through every attempt and backoff sleep, every newly
acknowledged head would wait behind them, and a wall-clock pending TTL would
drop acknowledged heads that were never fetched although their senders ended
their scope markers on the success reply.

- The module adds only state the code holds; its header maps each variable
  to its Rust field. `dueOrder` and `backoff` are `PendingDag::next_retry_at`
  kept by its order, the only thing the code reads from it: `dueOrder` holds
  the roots whose deadline has passed, in `next_retry_at` order (what
  `due_pending_dag_retries` returns), and `backoff` the roots whose deadline
  is still ahead; `BecomeDue` is a deadline passing, in any order. `tries` is
  the loop index of `poll_fetch_dag`'s attempts. An acknowledged root is due at once
  (`next_retry_at = now` at registration, and at restore). `WClaim` is
  `ClaimFetch` restricted to a due root, and to the oldest due root that could
  take a slot (`dispatch_due_pending_dag_fetches`); a root still in backoff is
  not due, so a root that became due later goes before it, as in the code.
  The claim moves the root into `backoff` (`try_claim_pending_dag_dispatch`
  sets `next_retry_at`). `INV_DueOrBackoff` checks the bookkeeping: an idle
  pending root is due or in backoff, never both, never neither.
- `AttemptMode = "PerAttempt"`: a clock-dispatched fetch makes one attempt and
  returns its slot (`poll_fetch_dag` with a pending lease). `"WholeTask"` is
  the old rule: `RetryInPlace` starts the next of `MaxAttempts` attempts while
  keeping the slot.
- `RearmMode = "AttemptEnd"`: the root's next deadline runs from the end of
  its attempt (`PendingDagLease::finish_attempt`, reached only through
  `DagFetchContext::finish_clock_attempt`, which does nothing for a
  reply-driven fetch: it has no lease and retries in place, outside this
  clock). So `BecomeDue` waits for the attempt to end. `"ClaimTime"` keeps the
  deadline the claim set: it can pass while the attempt runs, and when the
  attempt fails the root is due ahead of every head acknowledged meanwhile.
- `ExpiryMode = "AckRetained"`: an acknowledged entry never expires
  (`PendingDag::is_expired`); it leaves on merge, quarantine or a head of its
  scope that links it and counts against `max_pending_dags` meanwhile, whose overflow
  nacks so the sender keeps its marker. `"WallClockTtl"` lets `Expire` drop an
  acknowledged root that has not been fetched. The model does not credit the
  durable resync: with a pending store it re-registers such a root within a
  minute, but with its backoff reset, its running fetch's lease invalidated and
  a new TTL; without one the head is gone.
- `Dead` names the scopes whose provider never serves a fetch: it answers
  busy, stalls or is gone, for good. `WComplete` never fires for them; their
  attempts only fail.
- `Trickle` names the scopes whose provider serves slowly but productively:
  every window delivers a block (`TrickleWindow`), so the attempt never
  stalls, but the DAG outlasts any quantum. `CutoffMode = "Quantum"`: such an
  attempt gives its slot back at `CLOCK_ATTEMPT_QUANTUM` (`Preempt`), keeping
  what it fetched, and its root is due again at once, behind every root
  already due (`FetchTaskEnd::Preempted`, `finish_attempt(preempted)`).
  `"Unbounded"`, the rule before, ends an attempt only on a window without
  progress, so a trickling attempt keeps its slot as long as its DAG takes.
- An attempt also needs a fetch permit of its source peer (`DagFetchLimiter`:
  one fewer per peer than the global limit, so one peer cannot hold them all).
  `SharedPeer` names the scopes one source peer announces, each with
  `PeerPermits` permits; every other scope's peer announces only it. Pull
  fetches of that peer (DocSync and BranchableSync replies) take its permits
  too, with no slot (`PullBegin`, at most `MaxPulls` at once; `PullEnd`).
  `PermitMode = "WithSlot"`: the clock takes a root's slot and a permit of its
  peer together (`DagFetchLimiter::try_reserve_peer` in
  `dispatch_due_pending_dag_fetches`; the `DagNeedsFetch` event carries the
  permit to the fetch), and a root whose peer has none free is not eligible:
  it is passed over and keeps its place in the due order. `"AfterSlot"`, the
  rule before, reserved the slot and let the fetch wait for the permit in
  `DagFetchLimiter::acquire` (`parked`: waiters on the peer's semaphore), so
  after a restart the first spoke's due roots held every slot, one of them
  waiting for a permit of that spoke, while the other peers' heads waited for
  a slot. `INV_NoSlotWaitsForPermit`: no slot is held by a fetch waiting for a
  permit while a root that could start at once waits for a slot. The
  limiter's global permit is not modelled: the slot pool and the global limit
  are the same number, so clock fetches alone never wait for one, and a fetch
  waits for one only while pull fetches hold it, in a FIFO queue every peer's
  root would join too.

The checks use two documents, one fetch slot, `MaxV = 1` and one restart. The
liveness run makes only the receiver fair, and only at the scheduler level:
the clock dispatches whenever some due root can take a slot (weak fairness
on some `WClaim`), and merges and their cleanup run. No root, scope or job
has dispatch fairness of its own: which root goes next is the due order's
choice, so per-root progress has to follow from FIFO, the quantum and
rotation. Granting it per root is the assumption that would hide a slot held
for ever. Per root there are only timers: a deadline passes (`BecomeDue`),
an attempt's windows end (`WFail`: each window is bounded), a quantum ends
(`Preempt`).

`LIVE_AckedRootMerged` rests on an environment assumption, stated as strong
fairness on some `WComplete`: a fetch that can complete eventually does.
Reachability alone does not give it, since a reachable provider can answer
busy every time. The assumption is a `Boundary` in `proofs/src/registry.rs`.
`Dead` and `Trickle` providers never complete in the model. Their heads stay
owned (`ACT_AckedRootLeavesOwned`) and keep being retried, but
`LIVE_AckedRootMerged` promises them nothing; it promises that they do not
stop the other heads from merging. `MC_SyncOwnership_Green_Dispatch_DeadProvider`
and `MC_SyncOwnership_Green_Dispatch_Trickle` check that; with the property
unrestricted, the dead-provider check fails, which is why the assumption is
needed. `MC_SyncOwnership_Red_UnboundedProductive` drops the quantum: a
trickling root claims the only slot and d2 is never dispatched.

The permit checks need a second peer. `MC_SyncOwnership_Green_Dispatch_PeerPermits`
uses three documents and two slots: one spoke announces d1 and d2 with one
permit, d1's provider trickles, and d3 comes from another peer. It checks
safety only, since its liveness run takes close to a minute;
`MC_SyncOwnership_Red_SlotBeforePermit` is the same configuration under
`"AfterSlot"`. `MC_SyncOwnership_Green_Dispatch_PullPermit` checks liveness
with two documents, one slot and a pull fetch that may hold d1's peer's only
permit: d1 is passed over while it does and still merges. A passed-over root
has no fairness of its own: its turn follows from the due order once a
permit comes back. The liveness runs also assume that pull fetches, which
answer this node's own sync requests, come in bursts that end: from some
point on none starts (`<>[][~PullBegin]_allVars` in `WFairSpec`). Without
that assumption, or without weak fairness on `PullEnd` (a pull fetch that
never ends), d1 is never dispatched and the liveness check fails, so the
result is not vacuous.

The clock does not start a fetch itself: it hands the root to the merge loop
(`DagNeedsFetch`, or `DagReady` for a merge continuation), which starts it.
The merge loop drains one bounded channel of merge work (`event_buffer_size`,
256), one event at a time. In a catch-up storm other senders keep that
channel full and wait for room: pull-sync replies, finished fetches
reporting `DagReady`, transport handlers that complete another root's DAG,
and the merge loop itself when it handles a block-sync completion that was
latched before it registered the query. A freed cell goes to the oldest
waiting sender, so `try_reserve` finds room only when nobody waits.

- `fifo` is that channel (`FifoCap` cells) and `blocked` the senders waiting
  for room in it, oldest first, both code state: the channel's buffer, with
  a finished fetch's `DagReady` kept by its root and other merge work only as
  `Work`, and the `Sender::send` calls awaiting their permits. `Storm` turns
  on `StormSend`, the environment's merge work; it gets no fairness and is
  never made to stop. `Drain` is the merge loop taking its next event, once
  the merge it is on is done, with weak fairness; `Admit` is a waiting
  sender taking the cell it is owed, with weak fairness. A fetch's outcomes
  need the merge loop to have taken its dispatch and the fetch to be still
  running (`Fetching`). A finished fetch reports its `DagReady` under its
  slot (`WComplete`): it waits for room like any other sender, oldest
  first, and gives its slot back once the report is queued. Its root merges
  where the merge loop takes that report (`Drain` claims the merge), not by
  a fairness of its own.
- `DispatchPath = "OwnLane"`: the clock's dispatches take a lane of their own
  (`lane`; `DispatchLane`, one per fetch slot, checked by `can_dispatch`),
  which the merge loop takes from before the channel
  (`SyncEventReceiver::recv`). No waiting sender is owed the lane's room.
  `"MergeFifo"`, the rule before, takes a cell of the channel with
  `try_reserve`.
- `LaneTurns = "Bounded"`: the lane goes first for at most as many
  dispatches in a row as it has slots (`turns`,
  `SyncEventReceiver::turns`); then waiting merge work is taken, and that
  gives the lane its turns back. A dispatch waits behind at most one event
  of merge work. `"Always"`, the rule before, lets roots the clock
  re-dispatches before every receive (a trickling attempt at its quantum, a
  local merge continuation) keep queued merge work waiting for good.
- `HandlerSend = "TryThenClock"`: a PushLog, peer-connect or completion
  handler that completes another root's DAG, and the merge loop doing so
  inline, try the channel once; when it is full they make the root due at
  once and the clock dispatches it (`retry_pending_dag`), so a handler never
  waits in its lane slot and the merge loop never waits on itself.
  `"Blocking"`, the rule before, waits for room: a handler while it holds the
  admission lane slot and the CID guard
  (`INV_AdmissionNeverWaitsOnMergeLoop`), the merge loop with nobody left to
  drain its channel (`INV_MergeLoopNeverWaitsOnItself`).

`MC_SyncOwnership_Green_Dispatch_Storm` checks one document across a restart
with the storm on: the head merges, no handler waits in its admission slot
and the merge loop never waits on itself.
`MC_SyncOwnership_Red_MergeFifoDispatch` puts the dispatch back in the
channel: the waiting sender takes every freed cell and the next one waits
again, so the head is never dispatched. It fails under `WStrongClockSpec`, a
strongly fair clock, so the starvation does not come from the clock looking
at the wrong moments. `MC_SyncOwnership_Red_BlockingHandlerSend` lets a
handler wait in its admission slot, and
`MC_SyncOwnership_Red_LoopWaitsOnItself` lets the merge loop wait for room
in its own channel. `MC_SyncOwnership_Green_Dispatch_LaneTurns` gives d1 a
trickling provider and two slots, so d1 is re-dispatched at every quantum
before the merge loop looks again; d2's `DagReady` still gets its turn and
d2 merges. `MC_SyncOwnership_Red_LaneAlwaysFirst` lets the lane always go
first, and d2's `DagReady` waits behind d1 for good. Pull-sync replies (`DocSyncReply`,
`BranchableSyncReply`) still wait for room in their completion lane slot;
they have no clock to hand their blocks to, and the model keeps them as
`StormSend("pull")`.

## In-memory pending map

`MC_SyncOwnership_Memory.tla` runs every action of this model with the
receiver's bounded in-memory map in front of `pending`. Only roots in memory
(`mem`) can be claimed. The durable set can be larger than the map: before the
one restart the map holds the whole durable set, after it `MemCap` entries,
which is an upgrade or a lower cap. The restart restores only what fits
(`Resync`). Roots of `Dead` scopes sort first in key order, the worst case for
a key-order sweep.

- As in the dispatch model, every variable is code state, mapped in the
  module header: `mem` and `conts` are the map's keys by `PendingDag::origin`,
  `dispatches` is `PendingDag::dispatches` (counted up to `RotateAfter`),
  `dueOrder` and `backoff` are `next_retry_at` by its order, and `cursor` is
  `pending_resync_cursor`.
- Admission has two tiers (`process_block_inner`). A new head takes an entry
  and its durable record (`MRegister`, `try_insert_pending_dag`). With the map
  full it takes the durable record alone and is acknowledged
  (`MRegisterDurable`); it waits like any durable root without an entry, the
  rotation below gives it one, and it holds its place in the store's key order
  like the others. `AdmissionMode = "MemoryOnly"`, the rule before, nacks it
  instead (`MNack`) while the store has room: the sender keeps its marker,
  parks the peer and offers it again, and with the map full of acknowledged
  roots whose providers never serve, no durable root waits, nothing gives an
  entry up, and the head is nacked for ever.
- Each sending peer (`PeerScopes`) holds at most `DurableQuota` durable roots
  (`durable_share_admits`, `max_persisted_pending_per_peer`, a quarter of the
  durable cap, as the map's per-peer quota is a quarter of the map), in either
  tier. A head that would take its peer past it is nacked; nothing else is,
  except at the store's own cap (`ProcessNack`). With no quota
  (`DurableQuota = Cap`) one peer's never-serving roots fill the store and
  every other peer's head is nacked for ever.
- `OverflowMode = "Rotate"`: a root whose `dispatches` reached `RotateAfter`
  (`PENDING_ROTATE_AFTER_DISPATCHES`; 1 in the configurations) gives its
  entry up once its attempt has ended, while the map is full and a durable
  root waits, keeping its record (`Yield`; `PendingDagLease::finish_attempt`).
  The sweep resumes after the root it
  admitted last (`records_after_resync_cursor`). `"KeyOrder"` yields but
  sweeps from the first key, re-admitting the root that just gave its entry up.
  `"Retained"`, the rule before, never gives an acknowledged entry up.
- Entries record their origin (`PendingOrigin`): `Push`, `Restored` or
  `LocalContinuation`. A pull-sync merge that yields its work budget leaves a
  local continuation (`PullYield`, `continue_merge`); it is never an
  acknowledged obligation (`INV_ContinuationsNeverAcked`), and a push for the
  same root takes its entry over. `ContinuationMode = "EndOnStop"`: the first
  turn that does not yield again (merged, a non-terminal skip, an error)
  removes the entry (`ContinuationStop`; `MergeTurn` dropping at the end of
  the turn). `"Retained"`, the rule before, keeps the entry, which the clock
  re-drives for ever while it counts against `max_pending_dags`.

`LIVE_AckedRootMerged` does not extend to local continuations, and is not
meant to: no sender waits on one, and a non-terminal skip waits on an input
the receiver does not own (a definition, an ACP registration). What holds is
that every continuation leaves the map (`LIVE_ContinuationEnds`) and that
ending one drops no acknowledged head. Its blocks stay local and unmerged;
the next pull or push of the head starts the merge again.

The checks use two documents (`d1` dead and first in key order, `d2` also
pulled), `MaxV = 1`, one fetch slot, a map of one after the restart, and the
dispatch model's scheduler-level fairness and environment assumption. Claims
follow `next_retry_at` order (`dueOrder`), as in the dispatch model, so no
root needs claim fairness of its own here either. Nothing is recorded only to
state a property: the end of a continuation is the liveness property
`LIVE_ContinuationEnds` over the map itself. The quota checks add a third
document: one peer pushes `d1` and `d2`, both dead, the other `d3`. Senders
get fairness per peer, never per scope: each peer's retry ledger offers one of
its dirty markers again, and the receiver answers one of that peer's hints, so
a peer may keep offering a nacked head instead of another.
`LIVE_WrittenHeadMerged` promises a merge only to scopes of peers with no
never-serving scope, since such a root can hold its peer's quota.

## Checked properties

- `INV_ObligationConservation`: when a receiver is behind the sender's current
  head, that exact current obligation is in sender durability, in flight,
  durably registered, or merged.
- `ACT_AckedWriteOwned`: a step that returns a commit call to its writer
  (`RegisterMarker`, or `SenderStop` dropping the registration unrun) leaves
  its head owed to the receiver, by the durable scope marker or a hint or
  durable registration of it or a newer head, unless already merged. A crash
  returns no commit call, so a head whose sender crashed before registering
  it was never acknowledged and is outside the promise.
- `INV_ConservedWhileSenderUp`: `INV_ObligationConservation`, counting a
  pending registration, while the sender runs. It does not hold across a
  crash between a commit and its registration.
- `INV_SingleFlight`: at most one fetch owner exists for an exact root/CID.
- `INV_FetchOwnersBounded`: scheduled and running fetch ownership never
  exceeds `FetchCap`, including the receiver-clock event handoff.
- `INV_SingleMergeWriter`: one receiver-owned writer may hold a batch of ready
  roots, but independent merge writers never overlap.
- `INV_SingleTerminalWriter`: repeated merge/quarantine observations for one
  root coalesce through one durable metadata writer, so idempotent cleanup
  cannot become concurrent OCC writers.
- `INV_FetchOwnerDrainsResponse`: first-block progress cannot truncate the CAR
  response that the sole fetch owner needs to complete the DAG.
- `INV_ReceiverQueueBounded`: durable want registrations never exceed `Cap`.
- `INV_PendingRetainsBoundProvider`: a live durable root retains the qualified
  recovery origin selected when ownership transferred.
- `INV_FetchHasQualifiedProvider`: an Iroh fetch cannot claim its single-flight
  owner until either the origin is routable or an authenticated direct
  complete-DAG offer has been durably admitted. `qualifiedProvider` abstracts
  a non-empty bounded candidate set; the exact three-alternate cap and
  principal identities are enforced by Rust conformance tests.
- `ACT_LinkingHeadRetiresScopeHead`: a head registered while its parent is
  the scope's current head atomically takes over that registration, so heads
  that arrive in order keep one durable obligation per scope.
- `INV_PendingServiceable`: every success-acked pending root retains a
  restart-safe CAR-serving path.
- `INV_PendingHasRoutableProvider`: every success-acked root retains the
  independently authenticated publisher through a reconnectable transport
  route. Continuous connection is not required; temporary unavailability
  preserves the durable obligation without recording terminal exhaustion.
- `INV_PendingHasAuthenticatedProvider`: a receiver never transfers durable
  ownership to a provider selected from an unsigned payload claim or an
  unverified relay.
- `INV_PendingHasCompleteProvider`: neither the durable source nor an alternate
  fetch candidate may be an authenticated gossip hop that owns only the root;
  linked-CID alternates require positive availability evidence or authenticated
  direct sender authority.
- `INV_PendingHasServingAuthorization`: a complete, routable origin must also
  be able to authorize the receiver at the block-serving boundary. With ACP
  enabled, endpoint authentication alone is insufficient: the endpoint must
  resolve the receiver's authenticated Defra identity (matching Go's identity
  challenge/token exchange), or an exact-root/configured replication grant
  must cover the request.
- `INV_KnownFrontierUsesSelective`: once the head block exposes the missing
  frontier, receiver work requests those exact CIDs instead of first walking
  and serializing the entire historical DAG.
- `INV_SenderMarkersOnly`: sender durable state contains no CID/version or
  payload delivery record.
- `INV_MarkersReplayable`: both document and collection scopes inhabit the same
  marker/rederive protocol.
- `LIVE_EventualCurrency`: under fair hint delivery and receiver dispatch, every
  scope eventually stays current despite drops, nacks, and one restart.
- `LIVE_EventualReceiverQuiescence`: currency is not enough; every merged durable
  receiver obligation is eventually retired.
- `ACT_AttemptsStartInDueOrder`, `ACT_EndedAttemptQueuesBehind` (dispatch),
  both properties of every step over `next_retry_at` order: an attempt starts
  only for the oldest due root that can run, and an attempt that ends leaves
  its root behind every root already waiting. Together a due root waiting for
  a fetch slot is never passed by a root that became due after it, so a head
  acknowledged after a restart is not starved by catch-up roots retrying.
  A root whose source peer has no fetch permit free cannot run and may be
  passed.
- `INV_NoSlotWaitsForPermit` (dispatch): no fetch slot is held by a fetch
  waiting for its source peer's permit while a root that could start at once
  waits for a slot. `INV_PeerPermitsBounded`: no peer has more permits in use
  than it has.
- `INV_DueOrBackoff` (dispatch): an idle pending root is either due or waiting
  out its backoff.
- `ACT_AckedRootLeavesOwned` (dispatch), a property of every step: an
  acknowledged head leaves the receiver only while someone else (sender
  marker, a hint, a newer pending head or the merge) still owns it.
- `LIVE_AckedRootMerged` (dispatch): every acknowledged head whose provider
  eventually serves is merged (the environment assumption above), however
  long a storm keeps the merge loop's channel full and however often the
  clock re-dispatches ahead of its `DagReady`.
- `INV_AdmissionNeverWaitsOnMergeLoop` (dispatch): no transport handler waits
  for room in the merge loop's channel while it holds an admission lane
  slot.
- `INV_MergeLoopNeverWaitsOnItself` (dispatch): the merge loop never waits
  for room in its own channel.
- `INV_MemoryBounded`, `INV_MemoryHoldsDurable` (memory): the map never holds
  more entries than it may, and an acknowledged entry always has its record.
- `INV_ContinuationsNeverAcked` (memory): a local continuation is never an
  acknowledged obligation.
- `LIVE_DurableRootMerged` (memory): every acknowledged durable head whose
  provider serves is merged, however many never-serving roots were restored
  ahead of it into a map too small for all of them.
- `LIVE_ContinuationEnds` (memory): every local continuation leaves the map.
- `INV_ForwardObligationOwned`: a head the hub merged and its downstream
  replicator lacks is still owed by the hub's durable downstream marker, by the
  upstream pending record of that head or a newer one, or by a running handler.
- `LIVE_EventualDownstreamCurrency`: every head the sender writes eventually
  reaches the hub's downstream replicator, despite drops, nacks and one hub
  restart. The forward and the downstream hop are fair at the scheduler level
  only.
- `INV_DurablePeerQuota` (memory): no peer holds more durable roots than its
  quota.
- `LIVE_WrittenHeadMerged` (memory): every head a sender writes, on a serving
  scope of a peer whose quota no never-serving root can hold, is merged: a
  full map never leaves it nacked, and owned by the sender, for ever.

The register-then-ack deviation from Go remains honest because `pending` is
durable in the green policy. An optimistic acknowledgement for an in-flight
fetch owner is deliberately not expressible.

## Configurations

| Configuration | Verdict | Boundary isolated |
| --- | --- | --- |
| `MC_SyncOwnership_Green.cfg` | GREEN | marker+rederive, durable registration, one fetch owner, current-head ack guard |
| `MC_SyncOwnership_Green_IrohOrigin.cfg` | GREEN | signed, transport-routable Iroh origin owns the complete linked DAG |
| `MC_SyncOwnership_Green_Liveness_IrohRelay.cfg` | GREEN | immutable signed origin plus durable qualified same-root alternate reaches currency |
| `MC_SyncOwnership_Green_Liveness_Doc.cfg` | GREEN | two-version document supersession eventually reaches currency and quiescence |
| `MC_SyncOwnership_Green_Liveness_Collection.cfg` | GREEN | two-version collection supersession eventually reaches currency and quiescence |
| `MC_SyncOwnership_Green_MarkerHandoff.cfg` | GREEN | a transactional commit returns only after its marker registration, P2P stops only after admitted writes drained, and a crash between commit and registration acknowledges nothing |
| `MC_SyncOwnership_Red_DocOnlyMarkers.cfg` | RED | a collection update is dropped after no collection marker was recorded |
| `MC_SyncOwnership_Red_PayloadLedger.cfg` | RED | current-main CID/payload-valued sender durability violates marker-only ownership |
| `MC_SyncOwnership_Red_VolatileRegistration.cfg` | RED | restart destroys the only state behind a success ack |
| `MC_SyncOwnership_Red_DuplicateFetch.cfg` | RED | two triggers claim the same root concurrently |
| `MC_SyncOwnership_Red_StaleAckClears.cfg` | RED | an old ack clears the marker for a newer head |
| `MC_SyncOwnership_Red_VolatileServeAuthority.cfg` | RED | sender restart loses the CAR authority behind a success-acked pending root |
| `MC_SyncOwnership_Red_RelayOnlyProvider.cfg` | RED | an unverified payload relay is recorded instead of a transport-authenticated recovery hop |
| `MC_SyncOwnership_Red_ProviderRebind.cfg` | RED | a same-root replay replaces the immutable recovery origin |
| `MC_SyncOwnership_Red_UnroutableOrigin.cfg` | RED | the recorded publisher has no direct-or-relayed CAR route, so durable receiver ownership cannot complete |
| `MC_SyncOwnership_Red_UnsignedIrohOrigin.cfg` | RED | an Iroh relay accepts an unsigned payload origin as if transport-authenticated |
| `MC_SyncOwnership_Red_RootOnlyHop.cfg` | RED | an authenticated gossip relay owns the root but cannot serve its linked descendants |
| `MC_SyncOwnership_Red_UnauthorizedOrigin.cfg` | RED | the authenticated origin owns the DAG but cannot authorize the receiver, so it serves metadata only |
| `MC_SyncOwnership_Red_CancelOnProgress.cfg` | RED | the receiver cancels a productive CAR stream after its first block, stranding descendants |
| `MC_SyncOwnership_Red_RecursiveFirst.cfg` | RED | a known missing frontier is delayed behind a recursive historical-root CAR walk |
| `MC_SyncOwnership_Red_EveryRoot.cfg` | RED | successive current heads from one sender/scope accumulate obsolete durable roots |
| `MC_SyncOwnership_Red_ParallelMerge.cfg` | RED | frontend-selected parallel merge workers overlap independent receiver writers |
| `MC_SyncOwnership_Red_DuplicateTerminal.cfg` | RED | two terminal observations concurrently delete the same durable pending root |
| `MC_SyncOwnership_Red_DischargeBeforeForward.cfg` | RED | the hub discharges a merged head's upstream record before its downstream marker is durable; a failed forward or a crash between loses the head for the downstream replicator |
| `MC_SyncOwnership_Red_OwedInMemory.cfg` | RED | the hub forwards in the terminal step only a root its running process remembers merging; a crash between the merge commit and the terminal step loses the head for the downstream replicator |
| `MC_SyncOwnership_Red_EdgeTriggeredCompletion.cfg` | RED | a fast CAR failure is discarded before the fetch owner registers its completion waiter |
| `MC_SyncOwnership_Red_WorkerSaturatedCompletion.cfg` | RED | saturated spawned workers stop draining the CAR completion required by an active fetch owner |
| `MC_SyncOwnership_Red_SharedServeWorkers.cfg` | RED | slow ownership registration consumes every worker needed to serve receiver-owned CAR recovery |
| `MC_SyncOwnership_Red_EagerIdentityLookup.cfg` | RED | a durable replicator grant waits behind an unnecessary reverse DID challenge until CAR timeout |
| `MC_SyncOwnership_Red_BlockingHostEvent.cfg` | RED | a full event channel blocks the host command needed to reply and release admitted work |
| `MC_SyncOwnership_Red_BusyExhaustion.cfg` | RED | a useful CAR coalesced behind an existing shared-CID storage owner is misclassified as terminal provider exhaustion instead of returning to the paced root clock |
| `MC_SyncOwnership_Green_Dispatch.cfg` | GREEN | one attempt per fetch slot, its next deadline from its end, acknowledged heads kept: due roots go in order and every acknowledged head merges across a restart |
| `MC_SyncOwnership_Green_Dispatch_DeadProvider.cfg` | GREEN | the same with `d1`'s provider never serving: `d1`'s head is kept, `d2`'s still merges |
| `MC_SyncOwnership_Green_Dispatch_Trickle.cfg` | GREEN | the same with `d1`'s provider trickling: `d1`'s attempt gives its slot back at its quantum and `d2`'s head still merges |
| `MC_SyncOwnership_Red_UnboundedProductive.cfg` | RED | no quantum: `d1`'s trickling attempt never ends, holds the only slot, and `d2` is never dispatched (`LIVE_AckedRootMerged`) |
| `MC_SyncOwnership_Red_HeldSlot.cfg` | RED | a fetch keeps its slot through all of its attempts and starts the next ahead of a head acknowledged while it held the slot (`ACT_AttemptsStartInDueOrder`) |
| `MC_SyncOwnership_Red_ClaimTimeRearm.cfg` | RED | the root's next deadline is fixed when it is claimed and passes during the attempt, so the failed root ends due ahead of heads acknowledged during its attempt (`ACT_EndedAttemptQueuesBehind`) |
| `MC_SyncOwnership_Red_AckedTtl.cfg` | RED | the pending TTL drops an acknowledged head whose sender already ended its marker (`ACT_AckedRootLeavesOwned`) |
| `MC_SyncOwnership_Green_Dispatch_PeerPermits.cfg` | GREEN | the clock takes a slot with a permit of the root's source peer: while d1's trickling attempt holds its spoke's only permit, d2 is passed over and the second slot goes to d3, from another peer |
| `MC_SyncOwnership_Green_Dispatch_PullPermit.cfg` | GREEN | a pull fetch holds d1's peer's only permit: d1 is passed over, d2 takes the slot, and both heads merge across a restart |
| `MC_SyncOwnership_Red_SlotBeforePermit.cfg` | RED | the slot is reserved first and the fetch waits for its peer's permit inside it: d2 waits in the second slot while d3, able to start at once, finds none (`INV_NoSlotWaitsForPermit`) |
| `MC_SyncOwnership_Green_Memory.cfg` | GREEN | durable roots past the in-memory cap take turns, so a never-serving root restored first does not keep the other from merging; a head written while the map is full is acknowledged on its durable record and merges; a local continuation ends when it stops yielding |
| `MC_SyncOwnership_Red_RetainedOverflow.cfg` | RED | an acknowledged root never gives its entry up: the never-serving root restored first holds the only entry for good (`LIVE_DurableRootMerged`) |
| `MC_SyncOwnership_Red_KeyOrderResync.cfg` | RED | the root gives its entry up but the sweep restarts at the first key and re-admits it ahead of the other (`LIVE_DurableRootMerged`) |
| `MC_SyncOwnership_Red_ImmortalContinuation.cfg` | RED | a local continuation whose turn skips keeps its entry and is re-driven for ever (`LIVE_ContinuationEnds`) |
| `MC_SyncOwnership_Red_UndrainedMarker.cfg` | RED | P2P stops with a transactional write's marker registration pending; the dropped task returns the commit, which is acknowledged with no marker owing its head |
| `MC_SyncOwnership_Green_TwoTierQuota.cfg` | GREEN | one peer's two never-serving scopes and another peer's serving scope: the first peer is held at its quota of one durable root, and the other's head is acknowledged in either tier and merges |
| `MC_SyncOwnership_Red_MemoryOnlyAdmission.cfg` | RED | a full map nacks `d2`'s head while the store has room; `d1` never gives its entry up because no durable root waits, so the head is nacked and offered again for ever and its marker stays with the sender (`LIVE_WrittenHeadMerged`) |
| `MC_SyncOwnership_Red_NoDurablePeerQuota.cfg` | RED | no per-peer quota: one peer's two never-serving roots fill the durable store and the other peer's head is nacked for ever (`LIVE_WrittenHeadMerged`) |
| `MC_SyncOwnership_Red_ShedAsStall.cfg` | RED | a provider that sheds a CAR request as busy is read as a stalled window, so repeated shedding by a saturated hub burns the attempt budget into terminal provider exhaustion. `ProviderShed` is the release of an attempt in which every provider shed one batch, or in which a provider shed and none delivered, including a busy hub beside alternates that do not hold the root; like `BusyExhaustion`, this guards the classification, and the provider mix itself is tested in `busy_provider_rotation.rs` |

Each red configuration checks only type safety plus the property it is meant to
violate, keeping its counterexample attributable.

## Runtime conformance fence

The deterministic ownership A/B integration test holds topology, logical DAG,
admission bounds, and transport constant while switching only sender delivery
shape. Both arms run a real receiving coordinator through pending registration,
CAR pull, merge, and quiescence. The current-policy arm demonstrates the
dependency-PushLog feedback for a collection DAG: the receiver safely stores
the legacy field PushLog as a descendant, the standalone composite dependency
consumes the fixed slot, the collection head receives the actionable capacity
nack, and the receiver does not reach the source's current head on the first
wave. The frozen sender then retains its logical-head marker, handles the nack,
retries/re-offers the root after the dependency obligation drains, and is
required to reach the same final state. The target arm witnesses one hint,
restart-safe rooted selective-CAR
authority, one receiver fetch owner, bounded pending and persisted high-water,
balanced registered/merged/quarantined terminal counts, zero retained handles
after shutdown, and a receiver head identical to the source without a sender
retry cycle. The test proves admission amplification and extra feedback work;
it does not claim a fair old sender is permanently unable to converge. The
delivery-shape-only worker fence remains separate and is not presented as
convergence evidence. Storage tests bind
document and collection marker migration, current-head rederivation, and stale
ack protection. `p2p_admission_restart` continues to bind durable
register-then-ack, and the mixed Go/Rust suite plus the Go-emitted PushLog byte
fixture bind wire compatibility.

Receiver terminal conformance treats the pending store as authoritative over
its bounded in-memory accounting cache. The resync snapshot shares the same
metadata writer as registration, terminal deletion, and quarantine, then
replaces (rather than unions) the cached root set. A terminal merge also clears
any older live pending entry for that exact root. Newly merged, batch-merged,
already-merged, and stale PushLog observations all enter the same idempotent
terminal reconciliation operation; raw live-map deletion is not exposed at the
coordinator boundary. Deterministic tests retain both fleet-observed red states:
a deleted durable record with a live pending entry must not resurrect persisted
accounting, and a root merged through another arrival path must leave neither
receiver representation live. These are runtime refinements of
`ClaimTerminal`/`CompleteTerminal`; they do not add a second cleanup clock or
change the model's legal ownership transitions.

The Iroh conformance suite also fans one multi-block head from a serviceable
origin to more receivers than the provider's fixed CAR worker reserve. Every
receiver must merge and retire both live and durable state with no terminal
fetch exhaustion. This isolates bounded DefraDB fan-out from downstream client
write/query amplification; it does not substitute for the full fleet gate.

The outbound backlog retains its queue-wide resident-byte admission cap as a
hard memory bound for variable-sized head blocks. This is not CID-valued
delivery state: bytes are counted only while a bounded head-hint job is resident
and are never persisted or used as a retry identity.

`issue1154_repro` adds the restart/liveness fence that the small A/B cannot: it
success-acks hundreds of durable receiver obligations, restarts the hub with a
one-root admission bound, and requires every current document to merge through
the preserved receiver clock. This catches both dormant reconnect schedules
and response-completion latency that can otherwise serialize the sender and
receiver retry ladders into a non-quiescing lockstep.

Transport completion is ordered behind durable CAR response handling for the
same query. Every node surface uses one bounded scheduling policy with explicit
classes. PushLog, DocSync, BranchableSync, and decoded sync gossip occupy the
ownership-admission worker set. CAR requests use a distinct fixed
recovery-serving set, so serialized durable registration cannot consume the
provider capacity needed to discharge an acknowledged receiver obligation.
Received blocks, replies, and terminal results use the fixed completion set,
while lightweight peer state drains inline. Each bounded request class returns
the same actionable overload result rather than creating a queue or waiting for
a slot. An Iroh `BitswapComplete` cannot overtake storage of the response that
made the query successful or remain queued behind request-worker saturation.
Exact-CID owners may reap once every requested CID is durable; partial responses
retain the transport-completion drain fence.
Completion registration is also race-free in the other direction: Iroh may
finish a failed query before `sync_blocks` returns its query ID to the fetch
task. The bounded completion tracker latches that result, and registration
consumes it exactly once instead of burning the 30-second watchdog. This is the
Rust analogue of Go's blockservice session, which owns request and completion
inside one call and therefore has no separate edge-triggered subscription.

Every Rust node surface uses the same bounded transport-event dispatcher, with
the existing transport channel as its sole resident queue. There is no second
priority/backlog queue and no entrypoint-local concurrency policy. Sync-request
classification applies the fixed ownership-admission, recovery-serving, and
completion bounds and their actionable overflow; no class borrows another
class's permit. Completion work
is also bounded, with no second resident queue: its upstream producers are the
already-bounded receiver fetch/reply paths, and the transport channel retains
backpressure if all completion owners are occupied. Searchable-
encryption artifacts and management operations retain their existing event
paths and are not modeled as DAG-recovery owners in stage 3. On libp2p, a
full event channel no longer blocks the host from servicing response commands:
the event forwarder services commands while waiting for channel capacity. This
breaks the bounded-channel cycle in which a handler waited for the host while
the host waited for that handler to drain another event.

The receiver's process queue remains the DAG ownership boundary: a CAR response
atomically claims its root and contained CIDs without waiting, and an
overlapping arrival coalesces behind the current owner. Completion preserves a
third, explicit `deferred` disposition for this local contention: it releases
the current fetch lease and returns the durable root to the one paced receiver
retry clock without consuming provider-failure attempts or incrementing
terminal exhaustion. Only the storage owner may publish success. This is the bounded Rust
refinement of Go's per-root `processQueue`: duplicate ingest cannot retain
dispatcher workers while waiting for the owner that needs that transport to
make progress.
The completion edge is emitted once, after the bounded storage-conflict retry
finishes; an intermediate retriable attempt is not a provider failure and
cannot start a second recovery path.
PushLog requests use the same non-waiting root claim. A duplicate may success-
ack only after observing an existing durable registration or terminal merge;
otherwise it receives an actionable in-flight nack and the sender's durable
marker clock re-offers it. Explicit replay does not get a hidden waiter lane.
The dispatcher saturation regression fills every admission worker and every
bounded rejection slot with deliberately stalled sinks, proves later tokens are
closed rather than spawning detached writes, and proves a terminal event still
drains. A second regression fills the admission set and proves a CAR request
still reaches its recovery-serving reserve; that reserve has its own bounded,
actionable overflow fence. The converse regression stalls durable completion
work and proves inbound CAR serving still proceeds. It also holds the event
channel open after completion and requires live
scheduler counts to return to zero. The libp2p host regression fills the real
256-event channel and proves a response command is serviced before event
capacity is released; completed command tasks are reaped by that same host loop
instead of retained until shutdown. The CAR regression proves a
duplicate neither writes nor reports success before the root owner's durable
storage.

Go parity has two distinct identity patterns. Direct Go PushLog streams replace
`SenderID` with the authenticated connection peer, derive that ID from the
included public key, clear `Signature`, re-encode canonical CBOR, and verify.
Go pubsub relies on native signed gossipsub instead of signing the PushLog
payload, but `go-libp2p-pubsub-rpc` currently forwards `ReceivedFrom` (the last
relay), not the signed original `GetFrom()` author. Stage 3 does not treat that
hop as the content author. The Iroh endpoint ID is the public key and
`OriginSignature` is cleared for canonical encoding, so the original hint is
verified independently. Durable recovery prefers that verified origin when it
is transport-routable. A relayed hint whose verified origin is not routable is
dropped; the authenticated propagation hop is never promoted into a durable
provider merely because it forwarded gossip. Provider rotation may add a peer
only after positive evidence for a CID on the missing frontier. The additive-field Go fixture
proves existing Go decoders continue to accept the core head hint without
confusing `SourcePeerID` with authenticated Go message metadata.

Go's block-serving filter also resolves an unknown requester's Defra identity
through a challenge/response protocol, verifies that the returned token is
bound to the local peer ID, and then evaluates document ACP. Rust libp2p has
the same resolver, while Iroh previously installed `AnonymousResolver`; that
made a complete signed origin return only universally readable metadata to a
relayed subscriber. The Iroh resolver uses its authenticated QUIC endpoint as
the peer binding and the same audience-bound Defra identity token before ACP
evaluation. Commit signatures remain content-authorship evidence; they do not
replace this requester identity binding. Verified endpoint-to-DID bindings use
a bounded, expiring positive cache, matching the established libp2p behavior
without serializing every CAR request behind a fresh network challenge.
Concurrent misses for one endpoint share one bounded in-flight challenge;
failed resolutions are shared only by those waiters and are never cached.
As in Go, durable replicator authorization short-circuits before this fallback:
a configured peer never waits for an unrelated reverse identity challenge to
receive its CAR. This ordering is a liveness boundary, because the CAR response
stream and the identity challenge otherwise share the same transport timeout.

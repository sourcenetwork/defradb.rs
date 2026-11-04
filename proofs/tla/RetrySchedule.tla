---- MODULE RetrySchedule ----
\* The story. A node owes one replicator peer some documents. Each document it
\* has not delivered yet keeps a durable marker, and all of the peer's markers
\* share one retry clock: a rung on the retry ladder and a deadline. When the
\* deadline passes, the retry sweep replays the peer's markers. A replay that
\* lands removes its marker; a pass that lands nothing climbs one rung, so the
\* next wait is longer, up to the last rung, which then repeats (Go's ladder:
\* 30 seconds doubling to 32 minutes). A pass stops at a replay that fails or
\* hangs, or is cut short by the per-pass cap with nothing failed; a pass cut
\* short leaves the clock as it was, apart from what its replays' acknowledgements
\* do. Meanwhile local writes push their heads live: a live push that lands is
\* a delivery, one that fails leaves its marker for the sweep.
\*
\* Who acts: local writes (a live push that lands or fails), the sweep (a pass
\* over a due peer), the wall clock (ticks toward the deadline), and the
\* network (the peer goes away and comes back; coming back makes the peer due
\* at once, on the rung it had).
\*
\* What must never happen:
\*   - a peer that has just taken a delivery leaves its markers waiting longer
\*     than the first rung (INV_DeliveredPeerDueAtFirstRung): otherwise a push
\*     that fails right after a live acknowledgement waits behind the rung that
\*     an earlier outage, replay timeout or never-landing document charged, up
\*     to the last one;
\*   - the first pass that fails after a delivery, live or replayed, waits
\*     longer than the second rung (INV_FirstFailureAfterDeliveryChargesSecondRung):
\*     otherwise a run of replays that landed, in passes cut short by the cap,
\*     leaves the next timeout to charge the old rung;
\*   - a peer that keeps failing, with no delivery and no reconnect since, is
\*     swept again sooner than its rung allows (INV_SilentPeerBacksOff):
\*     otherwise every local write against a peer that is down would buy
\*     another early retry, a retry storm.
\*
\* Every promise is safety; no action is given fairness. Time passes only
\* while the peer is not due: the sweep runs every two seconds, so a due peer
\* is swept before a rung's worth of time can pass. Which marker a bounded pass
\* reaches first (the durable dispatch cursor) and receiver backpressure are
\* outside this model: it bounds when the peer is next swept, not the order
\* within a pass. SyncOwnership.tla gives each marker's re-hint weak fairness;
\* this model is the clock behind that re-hint.
\*
\* Code it models:
\*   crates/storage/src/stores/retry_info.rs  RetryInfo::bump_with_schedule (one rung up),
\*                                            RetryInfo::restart_after_delivery (back to the first rung, never later)
\*   crates/storage/src/stores/peerstore.rs   RegisterScopeMarkerAttempt (a new marker keeps the clock),
\*                                            PeerstoreRetryAttempt::CompleteMarker (an acknowledged completion restarts it),
\*                                            PeerstoreRetryAttempt::{Reschedule, Restart, Activate}
\*   crates/p2p-adapter/src/retry.rs          run_retry_pass (one rung per failed pass, the first rung after progress,
\*                                            nothing when MAX_MARKERS_PER_PEER_PASS or MAX_PEER_PASS cuts it short),
\*                                            record_failure (live acknowledgements and failures)
\*   crates/db/src/merge/push_docs_common.rs  complete_document_retry_if_current (replay acknowledgements)
\* Rust tests: crates/p2p-adapter/src/retry_sweep_tests.rs
\*   a_live_ack_returns_an_escalated_peer_to_the_first_rung,
\*   a_truncated_pass_that_delivered_returns_the_peer_to_the_first_rung,
\*   a_failed_live_push_alone_keeps_the_escalated_rung.
EXTENDS Naturals, Sequences

CONSTANTS
  Ladder,    \* the retry ladder in clock ticks, shortest first; its last rung repeats
  Scopes,    \* the documents the node pushes to the peer
  Poison,    \* documents whose push never lands, live or replayed
  RestartOn  \* what returns the peer to the first rung, besides a failed pass that landed some markers:
             \*   "LiveAck" and "ReplayAck" (a delivery, live or replayed: the fix, one rule in CompleteMarker),
             \*   "Register" or "Activate" (no delivery: mutants)

\* The ladder has at least one rung, and every rung waits some time.
ASSUME Ladder \in Seq(Nat \ {0}) /\ Len(Ladder) >= 1
\* Rungs never get shorter as the ladder climbs.
ASSUME \A i \in 1..(Len(Ladder) - 1) : Ladder[i] <= Ladder[i + 1]
\* Never-landing documents are some of the documents.
ASSUME Poison \subseteq Scopes
\* The restart knob names only these events.
ASSUME RestartOn \subseteq {"LiveAck", "ReplayAck", "Register", "Activate"}

\* The last rung's number.
Top == Len(Ladder)
\* The smaller of two numbers.
Min(a, b) == IF a <= b THEN a ELSE b
\* The wait a failed pass charges a clock that has made r retries: the r-th
\* rung counting from zero, or the last rung past the end (RetrySchedule::cap).
Wait(r) == Ladder[Min(r + 1, Top)]
\* The first rung's wait.
First == Ladder[1]
\* The last rung's wait, the longest there is.
Longest == Ladder[Top]

VARIABLES
  up,       \* the peer is reachable
  markers,  \* documents with a durable retry marker for the peer
  rung,     \* retries the peer clock has charged (num_retries), saturated at the last rung
  wait,     \* ticks until the peer clock's deadline; 0 means the peer is due
  proven,   \* bookkeeping: the peer took a delivery since its last failed pass
  failed,   \* bookkeeping: passes failed in a row since the last delivery, saturated at the last rung
  woke,     \* bookkeeping: the peer reconnected since its last failed pass
  slept,    \* bookkeeping: ticks since the peer's last pass
  relapsed  \* bookkeeping: the peer's last failed pass came right after a delivery

\* Every variable.
vars == <<up, markers, rung, wait, proven, failed, woke, slept, relapsed>>

\* The peer is reachable, nothing is owed, and there is no clock yet.
Init ==
  /\ up = TRUE
  /\ markers = {}
  /\ rung = 0
  /\ wait = 0
  /\ proven = FALSE
  /\ failed = 0
  /\ woke = FALSE
  /\ slept = 0
  /\ relapsed = FALSE

\* The peer becomes unreachable.
Disconnect ==
  \* It was reachable,
  /\ up
  \* and now is not.
  /\ up' = FALSE
  \* Nothing else changes.
  /\ UNCHANGED <<markers, rung, wait, proven, failed, woke, slept, relapsed>>

\* The peer comes back. Its connection event makes the clock due at once
\* (Activate); the rung stays unless the Activate mutant restarts it.
Reconnect ==
  \* It was unreachable,
  /\ ~up
  \* and now is reachable again.
  /\ up' = TRUE
  \* Its markers are due now.
  /\ wait' = 0
  \* The rung is kept; the mutant drops it to the first.
  /\ rung' = IF "Activate" \in RestartOn /\ markers # {} THEN Min(rung, 1) ELSE rung
  \* Remember the reconnect: it is evidence of a sort, not a delivery.
  /\ woke' = TRUE
  \* Nothing else changes.
  /\ UNCHANGED <<markers, proven, failed, slept, relapsed>>

\* A local write to s whose live push fails: its marker stays for the sweep.
\* The first marker creates the clock one rung up (RegisterScopeMarkerAttempt
\* bumps a new schedule); a later one leaves the clock as it is, unless the
\* Register mutant restarts it.
PushFails(s) ==
  \* s is owed to the peer.
  /\ markers' = markers \cup {s}
  /\ IF markers = {}
     \* A new clock: one retry charged, due after the first rung.
     THEN /\ rung' = 1
          /\ wait' = First
          /\ slept' = 0
     ELSE IF "Register" \in RestartOn
          \* The mutant: the new marker restarts the clock, with no delivery.
          THEN /\ rung' = Min(rung, 1)
               /\ wait' = Min(wait, First)
               /\ UNCHANGED slept
          \* The clock is kept.
          ELSE UNCHANGED <<rung, wait, slept>>
  \* No delivery happened, and the peer did not move.
  /\ UNCHANGED <<up, proven, failed, woke, relapsed>>

\* A local write to s whose live push lands: a delivery. The acknowledgement
\* clears s's marker (CompleteMarker). Clearing the last marker deletes the
\* clock; otherwise the fix returns the clock to the first rung, never later
\* than it already was (restart_after_delivery).
PushLands(s) ==
  \* Only a reachable peer acknowledges,
  /\ up
  \* and only a document that can land.
  /\ s \notin Poison
  \* s is no longer owed.
  /\ markers' = markers \ {s}
  /\ IF markers' = {}
     \* No marker left: the clock is deleted.
     THEN /\ rung' = 0
          /\ wait' = 0
          /\ slept' = 0
     ELSE IF "LiveAck" \in RestartOn
          \* The fix: at most the first rung, and no later than the deadline.
          THEN /\ rung' = Min(rung, 1)
               /\ wait' = Min(wait, First)
               /\ UNCHANGED slept
          \* The old rule: the escalated clock is kept.
          ELSE UNCHANGED <<rung, wait, slept>>
  \* Either way the peer has taken a delivery,
  /\ proven' = TRUE
  \* which ends its run of failed passes.
  /\ failed' = 0
  \* The peer did not move.
  /\ UNCHANGED <<up, woke, relapsed>>

\* One tick of the wall clock toward the deadline. A due peer is swept first.
Tick ==
  \* There is a clock,
  /\ markers # {}
  \* and it is not due yet.
  /\ wait > 0
  \* One tick closer to due,
  /\ wait' = wait - 1
  \* one tick longer since the last pass.
  /\ slept' = slept + 1
  \* Nothing else changes.
  /\ UNCHANGED <<up, markers, rung, proven, failed, woke, relapsed>>

\* A failed pass: one rung up (Reschedule: bump_with_schedule at the current rung).
Charge ==
  \* One more retry charged, up to the last rung.
  /\ rung' = Min(rung + 1, Top)
  \* The wait the current rung charges.
  /\ wait' = Wait(rung)
  \* The peer has failed since its last delivery,
  /\ proven' = FALSE
  \* once more in a row,
  /\ failed' = Min(failed + 1, Top)
  \* with no reconnect since,
  /\ woke' = FALSE
  \* right after a delivery, if the peer had just taken one,
  /\ relapsed' = proven
  \* and its pass just happened.
  /\ slept' = 0

\* The sweep reaches the due peer (run_retry_pass).
Pass ==
  \* There is a clock,
  /\ markers # {}
  \* and it is due.
  /\ wait = 0
  /\ IF ~up
     \* Not connected: a redial, and one rung up.
     THEN /\ Charge
          /\ UNCHANGED <<up, markers>>
     \* Connected: markers are replayed in turn, and any of the documents that
     \* can land may land. The pass then stops on a replay that fails or hangs,
     \* or the cap cuts it short with nothing failed.
     ELSE \E landed \in SUBSET (markers \ Poison), stopped \in BOOLEAN :
            \* The landed documents are no longer owed.
            /\ markers' = markers \ landed
            \* The peer did not move.
            /\ UNCHANGED up
            /\ IF markers' = {}
               \* Everything landed: the clock is deleted (finish_peer).
               THEN /\ rung' = 0
                    /\ wait' = 0
                    /\ proven' = TRUE
                    /\ failed' = 0
                    /\ woke' = FALSE
                    /\ slept' = 0
                    /\ UNCHANGED relapsed
               ELSE IF stopped
                    \* A replay failed or hung (failed in run_retry_pass).
                    THEN IF landed # {}
                         \* Some landed: back to the first rung (Restart).
                         THEN /\ rung' = 1
                              /\ wait' = First
                              /\ proven' = TRUE
                              /\ failed' = 0
                              /\ woke' = FALSE
                              /\ slept' = 0
                              /\ UNCHANGED relapsed
                         \* Nothing landed: one rung up.
                         ELSE Charge
                    \* Cut short with nothing failed: the pass itself leaves the
                    \* clock due, and only the replays' acknowledgements move it.
                    ELSE \* The cap is reached only after replays, and none failed, so some landed.
                         /\ landed # {}
                         \* The fix: each acknowledgement drops the clock to the first rung at most.
                         \* The old rule: the escalated rung is kept.
                         /\ rung' = IF "ReplayAck" \in RestartOn THEN Min(rung, 1) ELSE rung
                         \* The peer has taken a delivery,
                         /\ proven' = TRUE
                         \* which ends its run of failed passes,
                         /\ failed' = 0
                         \* and its pass just happened.
                         /\ slept' = 0
                         \* It is still due, and nothing else changes.
                         /\ UNCHANGED <<wait, woke, relapsed>>

\* Any one step: the network, the clock, the sweep, or a write's live push.
Next ==
  \/ Disconnect
  \/ Reconnect
  \/ Tick
  \/ Pass
  \/ \E s \in Scopes : PushFails(s) \/ PushLands(s)

\* Every behavior from the start.
Spec == Init /\ [][Next]_vars

\* Promise: every variable keeps its shape.
TypeOK ==
  /\ up \in BOOLEAN
  /\ markers \subseteq Scopes
  /\ rung \in 0..Top
  /\ wait \in 0..Longest
  /\ proven \in BOOLEAN
  /\ failed \in 0..Top
  /\ woke \in BOOLEAN
  /\ slept \in 0..Longest
  /\ relapsed \in BOOLEAN

\* Promise: once the peer has taken a delivery, and until one of its passes
\* fails, its markers are due within the first rung.
INV_DeliveredPeerDueAtFirstRung ==
  (proven /\ markers # {}) => wait <= First

\* Promise: a peer that has failed `failed` passes in a row, with no delivery
\* and no reconnect since, waits at least the `failed`-th rung after its last
\* pass before it is swept again.
INV_SilentPeerBacksOff ==
  (markers # {} /\ ~proven /\ ~woke /\ failed >= 1) => slept + wait >= Ladder[failed]

\* Promise: the first pass that fails after a delivery charges at most the
\* second rung. The delivery put the clock on the first rung, so that failure
\* climbs to the next one, not to the rung earlier failures earned.
INV_FirstFailureAfterDeliveryChargesSecondRung ==
  (relapsed /\ failed = 1 /\ markers # {}) => slept + wait <= Ladder[Min(2, Top)]
====

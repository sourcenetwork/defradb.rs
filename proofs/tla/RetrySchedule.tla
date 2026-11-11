---- MODULE RetrySchedule ----
\* The story. A node owes one replicator peer some documents. Each document it
\* has not delivered yet keeps a durable marker, and all of the peer's markers
\* share one retry clock: a rung on the retry ladder and a deadline. When the
\* deadline passes, the retry sweep replays the peer's markers. A replay that
\* lands removes its marker; a pass that lands nothing climbs one rung, so the
\* next wait is longer, up to the last rung, which then repeats (Go's ladder:
\* 30 seconds doubling to 32 minutes). A pass stops at a replay that fails or
\* hangs, or is cut short by the per-pass cap with nothing failed; a pass cut
\* short leaves the clock as it was, apart from what its replays'
\* acknowledgements do. Meanwhile a local write registers its head's marker
\* and pushes the head live: a live push that lands is a delivery and clears
\* the marker, one that fails leaves it for the sweep. A newer write to a
\* document whose push is in flight registers a newer marker and queues its
\* head behind that push; if the older push then lands, the newer marker stays,
\* and the acknowledgement is still a delivery. The deadline is wall-clock
\* time, and the wall clock can step back.
\*
\* Who acts: local writes and their live pushes, the sweep (a pass over a due
\* peer), the wall clock (ticks toward the deadline, or a step back), and the
\* network (the peer goes away and comes back; coming back makes the peer due
\* at once, on the rung it had).
\*
\* What must never happen:
\*   - a delivery that leaves markers leaves the clock above the first rung,
\*     or due later than the first rung (ACT_DeliveryRestarts): otherwise a
\*     push that fails right after an acknowledgement, or the timeout after a
\*     pass the cap cut short, waits behind the rung an earlier outage, replay
\*     timeout or never-landing document charged, up to the last one. Only a
\*     failed pass climbs, one rung at a time, so the first failure after a
\*     delivery charges at most the second rung;
\*   - while markers are owed, the rung falls without a delivery, or the
\*     deadline comes closer other than by time passing, a pass, a reconnect
\*     or a delivery (ACT_SilentPeerBacksOff): otherwise every local write
\*     against a peer that is down would buy another early retry, a retry
\*     storm;
\*   - a peer that is not due waits longer than the longest rung
\*     (INV_HeldWithinLongestRung): otherwise a backward step of the wall
\*     clock holds a connected peer for as long as the step.
\*
\* Every promise is safety; no action is given fairness. Time passes only
\* while the peer is not due: the sweep runs every two seconds, so a due peer
\* is swept before a rung's worth of time can pass. Which marker a bounded pass
\* reaches first (the durable dispatch cursor) and receiver backpressure are
\* outside this model: it bounds when the peer is next swept, not the order
\* within a pass. SyncOwnership.tla gives each marker's re-hint weak fairness;
\* this model is the clock behind that re-hint.
\*
\* State, and the code state each variable is (there is no other):
\*   up       the transport lists the peer as connected (connected_peers in run_retry_pass)
\*   markers  the documents with a ReplicatorRetryDocIDKey for the peer; generations are abstracted
\*   rung     RetryInfo::num_retries; every count from the last rung up waits the last rung
\*            (RetrySchedule::cap), so it stops there
\*   wait     RetryInfo::next_retry_unix less the wall clock, in ticks, or 0 once it has passed
\*   sending  the documents with a PushBacklog job for the peer in flight; the head queued
\*            behind a finished one goes out next, and the model sends it at once
\*   stale    the documents in sending whose PushBacklog `latest` head is newer than the one
\*            in flight (PushBacklog::is_current is false for it): a newer marker generation
\*            fences its acknowledgement
\* A peer with no ReplicatorRetryIDKey has rung = wait = 0.
\*
\* Code it models:
\*   crates/storage/src/stores/retry_info.rs  RetryInfo::bump_with_schedule (one rung up),
\*                                            RetryInfo::restart_after_delivery (back to the first rung, never later),
\*                                            RetryInfo::bounded_by (a deadline past the longest rung is due)
\*   crates/storage/src/stores/peerstore.rs   RegisterScopeMarkerAttempt (a new marker keeps the clock),
\*                                            PeerstoreRetryAttempt::CompleteMarker (an acknowledged completion
\*                                            restarts it, whether or not its generation is current),
\*                                            PeerstoreRetryAttempt::{Reschedule, Restart, Activate},
\*                                            Peerstore::load_scope_markers (the deadline the sweep reads)
\*   crates/p2p-adapter/src/retry.rs          run_retry_pass (one rung per failed pass, the first rung after progress,
\*                                            nothing when MAX_MARKERS_PER_PEER_PASS or MAX_PEER_PASS cuts it short),
\*                                            record_failure (live acknowledgements and failures)
\*   crates/p2p/src/sync/coordinator/push_worker.rs  run_push_job (a superseded push reports its acknowledgement)
\*   crates/db/src/merge/push_docs_common.rs  complete_document_retry_if_current (replay acknowledgements)
\* Rust tests: crates/p2p-adapter/src/retry_sweep_tests.rs
\*   a_live_ack_returns_an_escalated_peer_to_the_first_rung,
\*   a_superseded_live_ack_returns_an_escalated_peer_to_the_first_rung,
\*   a_truncated_pass_that_delivered_returns_the_peer_to_the_first_rung,
\*   a_failed_live_push_alone_keeps_the_escalated_rung;
\* crates/p2p/src/sync/coordinator/push_worker_tests.rs superseded_acknowledged_push_reports_its_delivery;
\* crates/storage/tests/retry_schedule.rs a_backward_clock_step_cannot_hold_a_ladder_deadline_past_its_longest_rung.
EXTENDS Naturals, Sequences

CONSTANTS
  Ladder,         \* the retry ladder in clock ticks, shortest first; its last rung repeats
  Scopes,         \* the documents the node pushes to the peer
  Poison,         \* documents whose push never lands, live or replayed
  RestartOn,      \* what returns the peer to the first rung, besides a failed pass that landed some markers:
                  \*   "LiveAck", "SupersededAck" and "ReplayAck" (a delivery, live, live after a newer
                  \*   write, or replayed: the fix, one rule in CompleteMarker),
                  \*   "Register" or "Activate" (no delivery: mutants)
  Steps,          \* how far the wall clock can step back, in ticks; none in the restart REDs
  BoundDeadlines  \* the sweep reads a deadline past the longest rung as due (RetryInfo::bounded_by)

\* The ladder has at least one rung, and every rung waits some time.
ASSUME Ladder \in Seq(Nat \ {0}) /\ Len(Ladder) >= 1
\* Rungs never get shorter as the ladder climbs.
ASSUME \A i \in 1..(Len(Ladder) - 1) : Ladder[i] <= Ladder[i + 1]
\* Never-landing documents are some of the documents.
ASSUME Poison \subseteq Scopes
\* The restart knob names only these events.
ASSUME RestartOn \subseteq {"LiveAck", "SupersededAck", "ReplayAck", "Register", "Activate"}
\* The clock steps back, if at all, by positive amounts.
ASSUME Steps \subseteq Nat \ {0}
\* The bound is on or off.
ASSUME BoundDeadlines \in BOOLEAN

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
\* The largest backward step, 0 if the clock never steps back.
MaxStep == IF Steps = {} THEN 0 ELSE CHOOSE m \in Steps : \A n \in Steps : n <= m

VARIABLES
  up,       \* the peer is connected
  markers,  \* documents with a durable retry marker for the peer
  rung,     \* retries the peer clock has charged (num_retries), stopped at the last rung
  wait,     \* ticks until the peer clock's deadline; 0 once it has passed
  sending,  \* documents with a live push to the peer in flight
  stale     \* documents in flight whose newer head waits behind the push

\* Every variable.
vars == <<up, markers, rung, wait, sending, stale>>

\* The sweep's due test on the deadline it reads: passed, or, with the bound,
\* further ahead than any rung writes one.
Due == wait = 0 \/ (BoundDeadlines /\ wait > Longest)

\* The peer is reachable, nothing is owed, and there is no clock yet.
Init ==
  /\ up = TRUE
  /\ markers = {}
  /\ rung = 0
  /\ wait = 0
  /\ sending = {}
  /\ stale = {}

\* After a delivery that leaves markers, under the rule for `event`: back to
\* the first rung, never later than the deadline (restart_after_delivery);
\* otherwise the clock is kept.
AfterDelivery(event) ==
  IF event \in RestartOn
  THEN /\ rung' = Min(rung, 1)
       /\ wait' = Min(wait, First)
  ELSE UNCHANGED <<rung, wait>>

\* The peer becomes unreachable.
Disconnect ==
  \* It was reachable,
  /\ up
  \* and now is not.
  /\ up' = FALSE
  \* Nothing else changes.
  /\ UNCHANGED <<markers, rung, wait, sending, stale>>

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
  \* Nothing else changes.
  /\ UNCHANGED <<markers, sending, stale>>

\* A local write to s registers its head's marker before the head is queued
\* for its live push (admit_replicator_push). The first marker creates the
\* clock one rung up (RegisterScopeMarkerAttempt bumps a new schedule); a later
\* one leaves the clock as it is, unless the Register mutant restarts it. A
\* push of s already in flight is superseded: the new head waits behind it.
Write(s) ==
  \* s is owed to the peer.
  /\ markers' = markers \cup {s}
  /\ IF markers = {}
     \* A new clock: one retry charged, due after the first rung.
     THEN /\ rung' = 1
          /\ wait' = First
     ELSE IF "Register" \in RestartOn
          \* The mutant: the new marker restarts the clock, with no delivery.
          THEN /\ rung' = Min(rung, 1)
               /\ wait' = Min(wait, First)
          \* The clock is kept.
          ELSE UNCHANGED <<rung, wait>>
  /\ IF s \in sending
     \* The push in flight is now superseded.
     THEN /\ stale' = stale \cup {s}
          /\ UNCHANGED sending
     \* The head goes out.
     ELSE /\ sending' = sending \cup {s}
          /\ UNCHANGED stale
  \* The peer did not move.
  /\ UNCHANGED up

\* The live push of s fails or times out. Its marker stays for the sweep and
\* the clock is kept (refresh_push_failure_with_generation); a superseded push
\* is retired with nothing recorded. Either way the newer head, if any, goes
\* out next.
PushFails(s) ==
  \* A push of s is in flight.
  /\ s \in sending
  /\ IF s \in stale
     THEN /\ stale' = stale \ {s}
          /\ UNCHANGED sending
     ELSE /\ sending' = sending \ {s}
          /\ UNCHANGED stale
  \* Nothing durable changes.
  /\ UNCHANGED <<up, markers, rung, wait>>

\* The live push of s lands: a delivery. Its acknowledgement clears s's marker
\* (CompleteMarker), unless the push was superseded: then the newer marker's
\* generation fences it, the marker stays, and the newer head goes out next.
\* Clearing the last marker deletes the clock; otherwise the delivery's rule
\* applies.
PushLands(s) ==
  \* Only a reachable peer acknowledges,
  /\ up
  \* a push of s is in flight,
  /\ s \in sending
  \* and s can land.
  /\ s \notin Poison
  /\ IF s \in stale
     \* Superseded: the newer head goes out, its marker stays.
     THEN /\ stale' = stale \ {s}
          /\ UNCHANGED <<sending, markers>>
     \* s is no longer owed.
     ELSE /\ sending' = sending \ {s}
          /\ markers' = markers \ {s}
          /\ UNCHANGED stale
  /\ IF markers' = {}
     \* No marker left: the clock is deleted.
     THEN /\ rung' = 0
          /\ wait' = 0
     ELSE AfterDelivery(IF s \in stale THEN "SupersededAck" ELSE "LiveAck")
  \* The peer did not move.
  /\ UNCHANGED up

\* One tick of the wall clock toward the deadline. A due peer is swept first.
Tick ==
  \* There is a clock, and it is not due yet.
  /\ markers # {}
  /\ ~Due
  \* One tick closer to due.
  /\ wait' = wait - 1
  \* Nothing else changes.
  /\ UNCHANGED <<up, markers, rung, sending, stale>>

\* The wall clock steps back d ticks, so the deadline is d ticks further off.
\* The model steps only from a deadline within the longest rung, which keeps
\* it finite; one step is enough to pass the bound.
ClockBack(d) ==
  \* There is a clock, within the longest rung.
  /\ markers # {}
  /\ wait <= Longest
  \* Its deadline is d ticks further off.
  /\ wait' = wait + d
  \* Nothing else changes.
  /\ UNCHANGED <<up, markers, rung, sending, stale>>

\* A failed pass: one rung up (Reschedule: bump_with_schedule at the current rung).
Charge ==
  \* One more retry charged, up to the last rung,
  /\ rung' = Min(rung + 1, Top)
  \* waiting the current rung from now.
  /\ wait' = Wait(rung)

\* The sweep reaches the due peer while it is unreachable: a redial, and one
\* rung up.
Redial ==
  \* There is a clock, it is due, and the peer is away.
  /\ markers # {}
  /\ Due
  /\ ~up
  \* One rung up.
  /\ Charge
  \* Nothing else changes.
  /\ UNCHANGED <<up, markers, sending, stale>>

\* The sweep replays the due, reachable peer's markers (run_retry_pass): those
\* in `landed` land, and the pass ends. Every marker landed, or a replay failed
\* or hung (`stopped`), or the cap cut the pass short with nothing failed.
Replay(landed, stopped) ==
  \* There is a clock, it is due, and the peer is reachable.
  /\ markers # {}
  /\ Due
  /\ up
  \* Only owed documents that can land do.
  /\ landed \subseteq markers \ Poison
  \* The landed documents are no longer owed.
  /\ markers' = markers \ landed
  /\ IF markers' = {}
     \* Everything landed: the clock is deleted (finish_peer).
     THEN /\ rung' = 0
          /\ wait' = 0
     ELSE IF stopped
          \* A replay failed or hung (failed in run_retry_pass).
          THEN IF landed # {}
               \* Some landed: back to the first rung (Restart).
               THEN /\ rung' = 1
                    /\ wait' = First
               \* Nothing landed: one rung up.
               ELSE Charge
          \* Cut short with nothing failed. The cap is reached only after
          \* replays, and none failed, so some landed; the pass itself leaves
          \* the clock, and only its replays' acknowledgements move it.
          ELSE /\ landed # {}
               /\ AfterDelivery("ReplayAck")
  \* Live pushes and the peer did not move.
  /\ UNCHANGED <<up, sending, stale>>

\* The sweep reaches the due peer.
Pass ==
  \/ Redial
  \/ \E landed \in SUBSET Scopes, stopped \in BOOLEAN : Replay(landed, stopped)

\* A step in which the peer took a delivery, live (superseded or not) or replayed.
Delivery ==
  \/ \E s \in Scopes : PushLands(s)
  \/ \E landed \in SUBSET Scopes, stopped \in BOOLEAN :
       landed # {} /\ Replay(landed, stopped)

\* Any one step: the network, the clock, the sweep, or a write's live push.
Next ==
  \/ Disconnect
  \/ Reconnect
  \/ Tick
  \/ \E d \in Steps : ClockBack(d)
  \/ Pass
  \/ \E s \in Scopes : Write(s) \/ PushLands(s) \/ PushFails(s)

\* Every behavior from the start.
Spec == Init /\ [][Next]_vars

\* Promise: every variable keeps its shape.
TypeOK ==
  /\ up \in BOOLEAN
  /\ markers \subseteq Scopes
  /\ rung \in 0..Top
  /\ wait \in 0..(Longest + MaxStep)
  /\ sending \subseteq Scopes
  /\ stale \subseteq sending

\* Promise: a delivery that leaves markers puts the clock on the first rung at
\* most, due within the first rung.
ACT_DeliveryRestarts ==
  [][(Delivery /\ markers' # {}) => (rung' <= 1 /\ wait' <= First)]_vars

\* Promise: while markers are owed, only a delivery lowers the rung, and only
\* time passing, a pass, a reconnect or a delivery brings the deadline closer.
ACT_SilentPeerBacksOff ==
  [][(markers # {} /\ markers' # {}) =>
       /\ (rung' < rung => Delivery)
       /\ (wait' < wait => (Tick \/ Pass \/ Reconnect \/ Delivery))]_vars

\* Promise: a peer that is not due is due within the longest rung.
INV_HeldWithinLongestRung ==
  (markers # {} /\ ~Due) => wait <= Longest
====

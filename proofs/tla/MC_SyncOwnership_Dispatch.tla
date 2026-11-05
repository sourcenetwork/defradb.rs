---- MODULE MC_SyncOwnership_Dispatch ----
\* The story. A receiver that acknowledged a head owes its sender a fetch of
\* that head's DAG: the success reply ended the sender's retry marker, so the
\* receiver is now the only one who remembers the head. The receiver's
\* per-root retry clock hands roots to a small pool of fetch slots (FetchCap).
\* Fetches can fail, the receiver can restart, and new heads keep arriving.
\* A root whose attempt ended waits out a backoff before it is due again,
\* some providers never serve at all, and some serve slowly: every window
\* delivers a block, but the DAG outlasts any wait we would accept.
\* Besides its slot, an attempt needs one of its source peer's fetch permits
\* (PeerPermits each), which pull fetches (DocSync and BranchableSync
\* replies) take too, without a slot.
\*
\* Who acts: the sender (writes and hints), the receiver's clock (claims a
\* slot for the oldest due root), each fetch (one attempt, which completes or
\* fails), the merge writer, a one-shot restart, the wall clock (a root's
\* retry deadline passes), and pull fetches of one source peer.
\*
\* What must never happen:
\*   - an attempt starts for any root but the oldest due root that can run
\*     (ACT_AttemptsStartInDueOrder), or an attempt ends with its root due
\*     ahead of a root that waited (ACT_EndedAttemptQueuesBehind): together, a
\*     root waiting for a slot is never passed by one that became due after
\*     it. Otherwise, after a restart, catch-up roots retrying can hold every
\*     slot and starve each newly acknowledged head;
\*   - a slot is held by a fetch waiting for its peer's permit while a root
\*     of another peer, which could start at once, waits for a slot
\*     (INV_NoSlotWaitsForPermit): otherwise one peer's due roots, first in
\*     the queue after a restart, keep every other peer's heads waiting;
\*   - an acknowledged head leaves the receiver while nobody else still owns
\*     it (ACT_AckedRootLeavesOwned): a wall-clock TTL would drop acknowledged
\*     heads whose senders have already forgotten them;
\* and what must eventually happen: every acknowledged head whose provider
\* serves it is fetched and merged (LIVE_AckedRootMerged). A head whose
\* provider never serves (Dead) or only trickles (Trickle) is kept but not
\* promised a merge here, and it does not keep the other heads from theirs:
\* a trickling attempt gives its slot back at its quantum (Preempt).
\* Fairness is the scheduler's only: the clock dispatches whenever it can and
\* some fetch that can complete eventually does. No root gets fairness of its
\* own for dispatch; its turn follows from the due order. Per root there are
\* only timers: its deadline passes, its attempt's windows end, its quantum
\* ends.
\*
\* This module adds the dispatch layer to SyncOwnership.tla without changing
\* it: every SyncOwnership step runs here too, followed by the bookkeeping
\* of the clock's queue. Every variable it adds is state the code holds; the
\* properties are stated over that state, as invariants or as properties of
\* each step, and nothing is recorded only to state them.
\*
\* Variables and the code state each one is (crates/p2p/src/sync):
\*   dueOrder  the pending roots whose PendingDag::next_retry_at (manager/pending.rs:81)
\*             has passed, in next_retry_at order: what due_pending_dag_retries
\*             (manager/process/pending_dag.rs:722) returns. next_retry_at is kept by
\*             its order, the only thing the code reads from it.
\*   backoff   the pending roots whose next_retry_at is still ahead: set by
\*             try_claim_pending_dag_dispatch (pending_dag.rs:562) and
\*             PendingDagLease::finish_attempt (pending_dag.rs:80). Which of them passes
\*             next is left open (BecomeDue), so every order of deadlines is explored.
\*   tries     the attempt a root's fetch task is on: the loop index of poll_fetch_dag
\*             (coordinator/dag_fetcher.rs:206), 0 when no task runs.
\*   parked    the roots whose clock-dispatched fetch task holds its reservation and
\*             waits in DagFetchLimiter::acquire (coordinator/mod.rs) for a permit of
\*             its source peer: waiters on that peer's Semaphore in
\*             DagFetchLimiter::per_peer. Empty unless PermitMode = "AfterSlot".
\*   pullHeld  the permits of SharedPeer's peer held by reply-driven fetches:
\*             poll_fetch_dag without a pending lease, spawned by the DocSync and
\*             BranchableSync reply handlers (coordinator/event_handler/doc_sync.rs,
\*             branchable_sync.rs).
\* A peer's free permits (its Semaphore's available_permits) are PeerPermits
\* less InUse below.
\* SyncOwnership's flights is the fetch reservation (SyncShutdownHandle's
\* pending_dag_fetch_tasks, coordinator/mod.rs) and pending the durable
\* records (PersistedView, pending_store/view.rs:27).
\*
\* Code it models (crates/p2p/src/sync):
\*   coordinator/mod.rs  dispatch_due_pending_dag_fetches (oldest due first, one slot each,
\*                        taken with a permit of the root's source peer),
\*                        DagFetchLimiter::try_reserve_peer, DagFetchLimiter::acquire
\*   coordinator/dag_fetcher.rs  poll_fetch_dag (a clock-owned fetch makes one attempt,
\*                                ended at CLOCK_ATTEMPT_QUANTUM while still progressing)
\*   replication/handlers.rs  handle_dag_needs_fetch (the slot comes back; the clock re-arms the root;
\*                             the permit the DagNeedsFetch event carries goes to the fetch)
\*   coordinator/dag_context.rs  DagFetchContext::finish_clock_attempt (only a clock-dispatched fetch re-arms)
\*   manager/process/pending_dag.rs  try_claim_pending_dag_dispatch, PendingDagLease::finish_attempt,
\*                                   due_pending_dag_retries (next_retry_at order, due roots only)
\*   manager/pending.rs  PendingDag::is_expired (only an unacknowledged entry expires)
\* Rust tests: tests/unit/restart_slot_fairness.rs, tests/unit/acked_pending_ttl.rs,
\*   tests/unit/productive_quantum.rs.
\*
\* The limiter's global permit is not modelled: the slot pool and the global
\* limit are the same number (max_concurrent_dag_fetches), so clock fetches
\* alone never wait for one, and a fetch waits for one only while pull
\* fetches hold it, in one queue that every peer's root would join too.
\* That queue is regolith's Semaphore (DagFetchLimiter::global), which is not
\* FIFO: an acquire may take a free permit past the waiters, but a waiter
\* passed over MAX_BYPASS times is owed the next one, so its wait ends.
EXTENDS SyncOwnership, Sequences

CONSTANTS
  AttemptMode, \* "PerAttempt": a fetch gives its slot back after one attempt | "WholeTask": the bug, it keeps the slot through every attempt
  RearmMode,   \* "AttemptEnd": a root's next deadline is set when its attempt ends | "ClaimTime": the bug, it is fixed when the root is claimed
  ExpiryMode,  \* "AckRetained": an acknowledged root never expires | "WallClockTtl": the bug, a TTL may drop it at any time
  MaxAttempts, \* attempts one fetch task makes before it gives up (MAX_FETCH_ATTEMPTS)
  Dead,        \* scopes whose provider never serves a fetch: it answers busy, stalls or is gone, for good
  Trickle,     \* scopes whose provider delivers a block every window, but whose DAG outlasts any quantum
  CutoffMode,  \* "Quantum": a productive attempt gives its slot back at its quantum | "Unbounded": the bug, it keeps it
  SharedPeer,  \* scopes whose heads one source peer announces; every other scope's peer announces only it
  PeerPermits, \* fetch permits per source peer (DagFetchLimiter's per-peer limit)
  MaxPulls,    \* pull fetches of SharedPeer's peer that run at once: each holds a permit and no slot
  PermitMode   \* "WithSlot": the clock takes a root's slot and its peer's permit together | "AfterSlot": the bug, the slot first, then the fetch waits for the permit

\* Each knob takes one of its named values.
ASSUME AttemptMode \in {"PerAttempt", "WholeTask"}
\* The re-arm knob takes one of its named values.
ASSUME RearmMode \in {"AttemptEnd", "ClaimTime"}
\* The expiry knob takes one of its named values.
ASSUME ExpiryMode \in {"AckRetained", "WallClockTtl"}
\* A fetch task makes at least one attempt.
ASSUME MaxAttempts \in Nat /\ MaxAttempts >= 1
\* Never-serving and trickling providers belong to some of the scopes.
ASSUME Dead \subseteq Scopes /\ Trickle \subseteq Scopes /\ Dead \cap Trickle = {}
\* The cutoff knob takes one of its named values.
ASSUME CutoffMode \in {"Quantum", "Unbounded"}
\* One fetch owner per root: the claim and the queue below assume it.
ASSUME FlightMode = "SingleFlight"
\* The shared peer announces some of the scopes, and every peer has a permit.
ASSUME SharedPeer \subseteq Scopes /\ PeerPermits \in Nat /\ PeerPermits >= 1
\* A bounded number of pull fetches.
ASSUME MaxPulls \in Nat
\* The permit knob takes one of its named values.
ASSUME PermitMode \in {"WithSlot", "AfterSlot"}

VARIABLES
  dueOrder,  \* roots whose next_retry_at has passed, in next_retry_at order
  backoff,   \* roots whose next_retry_at is still ahead
  tries,     \* [Roots -> 0..MaxAttempts], the attempt a root's fetch task is on
  parked,    \* roots holding a slot whose fetch waits for a permit of its peer
  pullHeld   \* permits of SharedPeer's peer held by pull fetches

\* The variables this module adds.
dvars == <<dueOrder, backoff, tries, parked, pullHeld>>
\* Every variable, SyncOwnership's and this module's.
allVars == <<vars, dvars>>

\* The roots a sequence holds.
Members(q) == {q[i] : i \in 1..Len(q)}
\* A sequence with the roots of S left out, the rest in order.
Without(q, S) == SelectSeq(q, LAMBDA r : r \notin S)
\* The roots of a finite set as a sequence, in one fixed order.
AsSeq(S) == CHOOSE f \in [1..Cardinality(S) -> S] :
              \A i, j \in 1..Cardinality(S) : i # j => f[i] # f[j]
\* The roots ahead of r in q (all of q when r is not in it).
Before(q, r) == IF r \in Members(q)
                THEN {q[i] : i \in 1..((CHOOSE k \in 1..Len(q) : q[k] = r) - 1)}
                ELSE Members(q)
\* Whether r comes ahead of q in q's sequence s (both in it).
Ahead(s, r, q) == r \in Members(s) /\ q \in Members(s) /\ r \in Before(s, q)

\* Scopes s and t are announced by the same source peer.
SamePeer(s, t) == s = t \/ (s \in SharedPeer /\ t \in SharedPeer)
\* Permits of scope s's source peer in use: one per fetch of that peer that
\* holds a slot and is not waiting for a permit, plus that peer's pull fetches.
InUse(s) ==
  Cardinality({r \in Roots : flights[r] > 0 /\ r \notin parked /\ SamePeer(r[1], s)})
    + (IF s \in SharedPeer THEN pullHeld ELSE 0)
\* Scope s's source peer has a permit free.
PeerFree(s) == InUse(s) < PeerPermits

\* A root the clock could hand a slot to if one were free: pending, servable,
\* with a reachable provider that owns its DAG, and no fetch running. Under
\* WithSlot its peer must also have a permit free, since the clock takes the
\* two together; under AfterSlot the clock does not look at permits.
Eligible(r) ==
  /\ r \in pending
  /\ (r \in serveAuth \/ r[1] \in serveScopes)
  /\ HasQualifiedProvider(r)
  /\ FetchMode = "RootThenSelective"
  /\ flights[r] = 0
  /\ (PermitMode = "AfterSlot" \/ PeerFree(r[1]))

\* Bookkeeping after any step, read off what the step did to pending and slots.
QueueStep ==
  \* Roots the step acknowledged (newly pending).
  LET added == pending' \ pending
      \* Roots the step retired (merged, superseded, expired).
      removed == pending \ pending'
      \* Roots the step gave a slot.
      claimed == {r \in Roots : flights[r] = 0 /\ flights'[r] > 0}
      \* Still-pending roots whose fetch gave its slot back in this step.
      released == {r \in pending' : flights[r] > 0 /\ flights'[r] = 0}
  IN
  \* An acknowledged root is due at once (next_retry_at = now); a claimed root
  \* leaves the due queue until its next deadline passes.
  /\ dueOrder' = Without(dueOrder, removed \cup claimed) \o AsSeq(added)
  \* The claim arms the root's next deadline; a retired root waits for nothing.
  /\ backoff' = (backoff \cup claimed) \ removed
  \* A claim starts attempt one; a released or retired root has no attempt.
  /\ tries' = [r \in Roots |-> IF r \in claimed THEN 1
                               ELSE IF r \in released \cup removed THEN 0
                               ELSE tries[r]]
  \* AfterSlot only: a root claimed while its peer has no permit free waits for
  \* one, holding its slot. A root whose slot went back, or that left, waits for nothing.
  /\ parked' = (parked \cup {r \in claimed : PermitMode = "AfterSlot" /\ ~PeerFree(r[1])})
                 \ (released \cup removed)
  \* Pull fetches start and end in steps of their own.
  /\ UNCHANGED pullHeld

\* A SyncOwnership step followed by the bookkeeping.
W(A) == A /\ QueueStep

\* A step of root <<s, v>>'s attempt, which starts only once its fetch holds
\* its peer's permit.
Attempt(s, v, A) == <<s, v>> \notin parked /\ W(A)

\* The clock claims a slot only for a due root, and only for the oldest due
\* root that could take one: a root in backoff is not due, so a root that
\* became due later can go before it. Under WithSlot the claim also takes a
\* permit of the root's peer (try_reserve_peer), so a root whose peer has none
\* free is passed over and keeps its place; under AfterSlot the fetch asks for
\* the permit only after the claim.
WClaim(s, v) ==
  /\ <<s, v>> \in Members(dueOrder)
  /\ ClaimFetch(s, v)
  /\ (PermitMode = "AfterSlot" \/ PeerFree(s))
  /\ \A q \in Before(dueOrder, <<s, v>>) : ~Eligible(q)
  /\ QueueStep

\* AfterSlot only: a peer's permit comes back and a fetch waiting for one
\* takes it, so its attempt can start.
Unpark(s, v) ==
  /\ <<s, v>> \in parked
  /\ PeerFree(s)
  /\ parked' = parked \ {<<s, v>>}
  /\ UNCHANGED <<vars, dueOrder, backoff, tries, pullHeld>>

\* A pull fetch of the shared peer (a DocSync or BranchableSync reply) takes
\* one of that peer's free permits, and no slot.
PullBegin ==
  /\ pullHeld < MaxPulls
  /\ \E s \in SharedPeer : PeerFree(s)
  /\ pullHeld' = pullHeld + 1
  /\ UNCHANGED <<vars, dueOrder, backoff, tries, parked>>

\* A pull fetch ends and gives its permit back.
PullEnd ==
  /\ pullHeld > 0
  /\ pullHeld' = pullHeld - 1
  /\ UNCHANGED <<vars, dueOrder, backoff, tries, parked>>

\* A failed attempt gives the slot back. Under WholeTask only the last of the
\* task's attempts does; the earlier ones retry in place (below). A trickling
\* provider delivers every window, so its attempts never stall.
WFail(s, v) ==
  /\ s \notin Trickle
  /\ <<s, v>> \notin parked
  /\ FailFetchBeforeWaiter(s, v)
  /\ (AttemptMode = "PerAttempt" \/ tries[<<s, v>>] >= MaxAttempts)
  /\ QueueStep

\* A completed fetch, which a provider that never serves never gives, and a
\* trickling one never gives within the horizon of this model.
WComplete(s, v) ==
  /\ s \notin Dead \cup Trickle
  /\ Attempt(s, v, CompleteFetch(s, v))

\* A trickling attempt's window delivers a block: progress, with nothing for
\* the clock to act on. Without a quantum, the attempt goes on like this.
TrickleWindow(s, v) ==
  /\ s \in Trickle
  /\ <<s, v>> \in pending
  /\ flights[<<s, v>>] > 0
  /\ <<s, v>> \notin parked
  /\ UNCHANGED allVars

\* Quantum only: an attempt still making progress reaches its quantum
\* (CLOCK_ATTEMPT_QUANTUM). It gives its slot back at the next window
\* boundary, keeping what it fetched, and its root is due again at once,
\* behind every root already due (FetchTaskEnd::Preempted, finish_attempt).
\* It is not a failure: no backoff, no exhaustion.
Preempt(s, v) ==
  /\ CutoffMode = "Quantum"
  /\ s \in Trickle
  /\ <<s, v>> \notin parked
  \* The slot is released like a deferral: no exhaustion, the record stays.
  /\ ContendedIngest(s, v)
  \* Due now, behind every root that was due before it.
  /\ dueOrder' = Append(Without(dueOrder, {<<s, v>>}), <<s, v>>)
  /\ backoff' = backoff \ {<<s, v>>}
  /\ tries' = [tries EXCEPT ![<<s, v>>] = 0]
  /\ UNCHANGED <<parked, pullHeld>>

\* The root's next_retry_at passes and the root is due again. AttemptEnd sets
\* that deadline when the attempt ends, so only a root with no fetch running
\* becomes due; ClaimTime fixed it at the claim, so it can pass while the
\* attempt still runs, and the root is ready the moment the attempt ends.
BecomeDue(s, v) ==
  \* The root is waiting out its backoff.
  /\ <<s, v>> \in backoff
  \* Under AttemptEnd its deadline runs from the end of its attempt.
  /\ RearmMode = "ClaimTime" \/ flights[<<s, v>>] = 0
  \* It is due now, behind every root that was due before it.
  /\ backoff' = backoff \ {<<s, v>>}
  /\ dueOrder' = Append(dueOrder, <<s, v>>)
  \* Nothing else changes.
  /\ UNCHANGED <<vars, tries, parked, pullHeld>>

\* WholeTask only: a failed attempt keeps its slot, sleeps, and starts the next
\* attempt without asking the clock, ahead of every root that is waiting.
RetryInPlace(s, v) ==
  \* Only the old hold-the-slot rule retries inside the task.
  /\ AttemptMode = "WholeTask"
  \* The root is still owed and its fetch holds a slot.
  /\ <<s, v>> \in pending
  /\ flights[<<s, v>>] > 0
  /\ <<s, v>> \notin parked
  \* The task has attempts left.
  /\ tries[<<s, v>>] < MaxAttempts
  \* The next attempt starts.
  /\ tries' = [tries EXCEPT ![<<s, v>>] = @ + 1]
  \* Nothing else changes: the slot stays with this root.
  /\ UNCHANGED <<vars, dueOrder, backoff, parked, pullHeld>>

\* WallClockTtl only: the TTL drops an acknowledged root that has not been
\* fetched yet, whether or not it was ever given a slot.
Expire(s, v) ==
  \* Only the bug lets the wall clock drop an acknowledged root.
  /\ ExpiryMode = "WallClockTtl"
  \* The root is owed and not merged.
  /\ <<s, v>> \in pending
  /\ mergedV[s] < v
  \* Its DAG is not complete yet, so no merge or cleanup owns it.
  /\ <<s, v>> \notin ready
  /\ mergeFlights[<<s, v>>] = 0
  /\ terminalFlights[<<s, v>>] = 0
  \* The receiver forgets the root and everything it held for it.
  /\ pending' = pending \ {<<s, v>>}
  /\ originBound' = originBound \ {<<s, v>>}
  /\ qualifiedOffers' = qualifiedOffers \ {<<s, v>>}
  /\ qualifiedProvider' = qualifiedProvider \ {<<s, v>>}
  /\ flights' = [flights EXCEPT ![<<s, v>>] = 0]
  /\ drained' = drained \ {<<s, v>>}
  /\ lostCompletions' = lostCompletions \ {<<s, v>>}
  \* The sender's state and the rest of the receiver are untouched.
  /\ UNCHANGED <<localV, dirty, payloadLedger, inflight, serveAuth, serveScopes,
                 ready, mergeFlights, terminalFlights, fetchExhausted, mergedV,
                 crashed, fwdVars>>
  /\ UNCHANGED senderVars
  \* The queues drop it too.
  /\ QueueStep

\* The restart: the resync sweep restores every durable root due at once, in
\* the sweep's order, with no fetch and no backoff. Fetches waiting for a
\* permit and pull fetches end with the process.
WCrash ==
  /\ Crash
  /\ dueOrder' = AsSeq(pending')
  /\ backoff' = {}
  /\ tries' = [r \in Roots |-> 0]
  /\ parked' = {}
  /\ pullHeld' = 0

\* Nothing pending, so nothing due or waiting, no fetch task and no permit held.
WInit ==
  /\ Init
  /\ dueOrder = <<>>
  /\ backoff = {}
  /\ tries = [r \in Roots |-> 0]
  /\ parked = {}
  /\ pullHeld = 0

\* Every SyncOwnership action, with the claim, the failure, the completion and
\* the restart replaced by their dispatch-aware forms, the attempt's other
\* endings only for a fetch that holds its permit, plus the deadline, the
\* trickling window, the quantum, the in-place retry, the TTL, a waiting fetch
\* taking its permit, and pull fetches.
WNext ==
  \/ \E s \in Scopes : W(Update(s)) \/ W(ReHintMarker(s))
  \/ \E s \in Scopes, v \in 1..MaxV :
       \/ W(ReHintPayload(s, v)) \/ W(DropHint(s, v)) \/ W(ProcessMerged(s, v))
       \/ W(ProcessRegister(s, v)) \/ W(ProcessNack(s, v))
       \/ W(SameRootReannounce(s, v))
       \/ W(OfferQualifiedAlternate(s, v)) \/ W(AddQualifiedAlternate(s, v))
       \/ WClaim(s, v) \/ WFail(s, v) \/ BecomeDue(s, v)
       \/ TrickleWindow(s, v) \/ Preempt(s, v)
       \/ RetryInPlace(s, v) \/ Expire(s, v)
       \/ Attempt(s, v, ContendedIngest(s, v)) \/ Attempt(s, v, ProviderShed(s, v))
       \/ Attempt(s, v, StarveTransportCompletion(s, v))
       \/ Attempt(s, v, StarveRecoveryServe(s, v))
       \/ Attempt(s, v, StarveRecoveryAuthorization(s, v))
       \/ WComplete(s, v) \/ W(ClaimMerge(s, v))
       \/ W(CompleteMerge(s, v)) \/ W(ClaimTerminal(s, v))
       \/ W(CompleteTerminal(s, v))
       \/ Unpark(s, v)
  \/ PullBegin \/ PullEnd
  \/ WCrash
  \/ W(Terminating)

\* Safety runs.
WSpec == WInit /\ [][WNext]_allVars

\* Liveness runs: once a head is acknowledged only the receiver acts on it, so
\* only the receiver is fair, and only at the scheduler: the clock dispatches
\* whenever it can, and merges and their cleanup run. No root has dispatch
\* fairness of its own; which root goes next is the due order's choice. Per
\* root there are only timers: a deadline passes, an attempt's windows end
\* (each is bounded), a quantum ends. Sender fairness is SyncOwnership's own
\* liveness configs. Failures, retries in place, the TTL and the restart are
\* never forced.
\*
\* Two environment assumptions. A fetch that can complete eventually does
\* (strong fairness on some WComplete). Reachability alone does not give it:
\* a provider can answer busy, or stall, every time. Dead and Trickle
\* providers never complete here; their heads stay owned
\* (ACT_AckedRootLeavesOwned), and LIVE_AckedRootMerged promises them nothing.
\* And pull fetches answer this node's own sync requests, so they come in
\* bursts that end: from some point on none starts (<>[][~PullBegin]).
\* A pull fetch ends (its windows are bounded) and a permit given back goes to
\* a fetch waiting for one; starting a pull fetch is never forced.
WFairSpec ==
  /\ WSpec
  /\ WF_allVars(\E s \in Scopes, v \in 1..MaxV : WClaim(s, v))
  /\ WF_allVars(\E s \in Scopes, v \in 1..MaxV : Unpark(s, v))
  /\ WF_allVars(PullEnd)
  /\ <>[][~PullBegin]_allVars
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : WComplete(s, v))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : W(ClaimMerge(s, v)))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : W(CompleteMerge(s, v)))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : W(ClaimTerminal(s, v)))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : W(CompleteTerminal(s, v)))
  /\ \A s \in Scopes :
       /\ WF_allVars(\E v \in 1..MaxV : BecomeDue(s, v))
       /\ WF_allVars(\E v \in 1..MaxV : WFail(s, v))
       /\ WF_allVars(\E v \in 1..MaxV : Preempt(s, v))

\* Promise: the added variables keep their shapes, on top of SyncOwnership's.
INV_DispatchTypeOK ==
  /\ TypeOK
  /\ dueOrder \in Seq(Roots)
  /\ backoff \subseteq Roots
  /\ tries \in [Roots -> 0..MaxAttempts]
  /\ parked \subseteq Roots
  /\ pullHeld \in 0..PeerPermits

\* Promise: a pending root with no fetch running is either due or waiting out
\* its backoff, never both and never neither, so no root is lost to the clock.
INV_DueOrBackoff ==
  \A r \in pending : flights[r] = 0 => ((r \in backoff) # (r \in Members(dueOrder)))

\* Someone still owns head r: it merged, the sender keeps its scope marker, a
\* hint for it or a newer head is in flight, or the receiver owes it or a newer head.
Owned(r) ==
  \/ mergedV[r[1]] >= r[2]
  \/ r[1] \in dirty
  \/ \E u \in inflight \cup pending : u[1] = r[1] /\ u[2] >= r[2]

\* Promise: an attempt starts only for the oldest due root that can run. An
\* attempt started any other way (the next one inside a task that kept its
\* slot) passes every root that is due and waiting.
ACT_AttemptsStartInDueOrder ==
  [][\A r \in Roots :
       tries'[r] > tries[r] =>
         \A q \in Members(dueOrder) \ {r} :
           Eligible(q) => (r \in Members(dueOrder) /\ q \notin Before(dueOrder, r))]_allVars

\* Promise: when an attempt ends, its root is not due ahead of a root that was
\* already waiting for a slot: the roots that waited during the attempt go
\* first. A restart restores every root due at once and is not an attempt end.
ACT_EndedAttemptQueuesBehind ==
  [][crashed' = crashed =>
       \A r \in Roots :
         (flights[r] > 0 /\ flights'[r] = 0 /\ r \in pending') =>
           \A q \in Members(dueOrder) \ {r} : Eligible(q) => ~Ahead(dueOrder', r, q)]_allVars

\* Promise: an acknowledged head leaves the receiver only while someone else
\* still owns it (the merge, the sender's marker, a hint or a newer head).
ACT_AckedRootLeavesOwned == [][\A r \in pending \ pending' : Owned(r)']_allVars

\* Promise: no peer has more permits in use than it has.
INV_PeerPermitsBounded == \A s \in Scopes : InUse(s) <= PeerPermits

\* Root r waits for a slot and could start its attempt the moment it had one:
\* it is due with no fetch, the clock could claim it, and its peer has a permit free.
CouldStart(r) == r \in Members(dueOrder) /\ Eligible(r) /\ PeerFree(r[1])

\* Promise: no slot is held by a fetch waiting for its peer's permit while a
\* root that could start at once waits for a slot.
INV_NoSlotWaitsForPermit == parked # {} => ~\E r \in Roots : CouldStart(r)

\* Promise: every acknowledged head whose provider serves is merged, whatever
\* never-serving or trickling heads hold the slots.
LIVE_AckedRootMerged ==
  \A r \in Roots : r[1] \notin Dead \cup Trickle => ((r \in pending) ~> (mergedV[r[1]] >= r[2]))
====

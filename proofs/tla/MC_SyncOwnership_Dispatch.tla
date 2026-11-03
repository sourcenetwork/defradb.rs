---- MODULE MC_SyncOwnership_Dispatch ----
\* The story. A receiver that acknowledged a head owes its sender a fetch of
\* that head's DAG: the success reply ended the sender's retry marker, so the
\* receiver is now the only one who remembers the head. The receiver's
\* per-root retry clock hands roots to a small pool of fetch slots (FetchCap).
\* Fetches can fail, the receiver can restart, and new heads keep arriving.
\* A root whose attempt ended waits out a backoff before it is due again,
\* some providers never serve at all, and some serve slowly: every window
\* delivers a block, but the DAG outlasts any wait we would accept.
\*
\* Who acts: the sender (writes and hints), the receiver's clock (claims a
\* slot for the oldest due root), each fetch (one attempt, which completes or
\* fails), the merge writer, a one-shot restart, and the wall clock (a
\* root's retry deadline passes).
\*
\* What must never happen:
\*   - a root that is due and waiting for a slot is passed by a root that
\*     became due after it (INV_ReadyRootsServedInOrder): otherwise, after a
\*     restart, catch-up roots retrying in place can hold every slot and
\*     starve each newly acknowledged head;
\*   - an acknowledged head leaves the receiver while nobody else still owns
\*     it (INV_ExpiryKeepsOwner): a wall-clock TTL would drop acknowledged
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
\* of the clock's queue.
\*
\* Code it models (crates/p2p/src/sync):
\*   coordinator/mod.rs  dispatch_due_pending_dag_fetches (oldest due first, one slot each)
\*   coordinator/dag_fetcher.rs  poll_fetch_dag (a clock-owned fetch makes one attempt,
\*                                ended at CLOCK_ATTEMPT_QUANTUM while still progressing)
\*   replication/handlers.rs  handle_dag_needs_fetch (the slot comes back; the clock re-arms the root)
\*   coordinator/dag_context.rs  DagFetchContext::finish_clock_attempt (only a clock-dispatched fetch re-arms)
\*   manager/process/pending_dag.rs  try_claim_pending_dag_dispatch, PendingDagLease::finish_attempt,
\*                                   due_pending_dag_retries (next_retry_at order, due roots only)
\*   manager/pending.rs  PendingDag::is_expired (only an unacknowledged entry expires)
\* Rust tests: tests/unit/restart_slot_fairness.rs, tests/unit/acked_pending_ttl.rs,
\*   tests/unit/productive_quantum.rs.
EXTENDS SyncOwnership, Sequences

CONSTANTS
  AttemptMode, \* "PerAttempt": a fetch gives its slot back after one attempt | "WholeTask": the bug, it keeps the slot through every attempt
  RearmMode,   \* "AttemptEnd": a root's next deadline is set when its attempt ends | "ClaimTime": the bug, it is fixed when the root is claimed
  ExpiryMode,  \* "AckRetained": an acknowledged root never expires | "WallClockTtl": the bug, a TTL may drop it at any time
  MaxAttempts, \* attempts one fetch task makes before it gives up (MAX_FETCH_ATTEMPTS)
  Dead,        \* scopes whose provider never serves a fetch: it answers busy, stalls or is gone, for good
  Trickle,     \* scopes whose provider delivers a block every window, but whose DAG outlasts any quantum
  CutoffMode   \* "Quantum": a productive attempt gives its slot back at its quantum | "Unbounded": the bug, it keeps it

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

VARIABLES
  dueOrder,   \* the clock's due queue: roots whose next_retry_at has passed, in next_retry_at order
  readyOrder, \* due roots without a fetch, in the order they became able to run
  backoff,    \* roots waiting for their next_retry_at
  tries,      \* [Roots -> 0..MaxAttempts], attempts the current fetch of a root has started
  overtaken,  \* roots that waited for a slot while a root behind them started an attempt
  expired     \* acknowledged roots the wall clock dropped

\* The variables this module adds.
dvars == <<dueOrder, readyOrder, backoff, tries, overtaken, expired>>
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

\* A root the clock could hand a slot to if one were free: pending, servable,
\* with a reachable provider that owns its DAG, and no fetch running.
Eligible(r) ==
  /\ r \in pending
  /\ (r \in serveAuth \/ r[1] \in serveScopes)
  /\ HasQualifiedProvider(r)
  /\ FetchMode = "RootThenSelective"
  /\ flights[r] = 0

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
      \* ClaimTime only: released roots whose deadline passed while they ran.
      overdue == released \cap Members(dueOrder)
  IN
  \* A root becomes ready (due, with no fetch) when it is acknowledged, when
  \* its deadline passes while it waits (BecomeDue), or, under ClaimTime, when
  \* its attempt ends after its deadline already passed.
  /\ readyOrder' = Without(readyOrder, removed \cup claimed) \o AsSeq(added \cup overdue)
  \* An acknowledged root is due at once (next_retry_at = now); a claimed root
  \* leaves the due queue until its next deadline passes.
  /\ dueOrder' = Without(dueOrder, removed \cup claimed) \o AsSeq(added)
  \* The claim arms the root's next deadline; a retired root waits for nothing.
  /\ backoff' = (backoff \cup claimed) \ removed
  \* A claim starts attempt one; a released or retired root has no attempt.
  /\ tries' = [r \in Roots |-> IF r \in claimed THEN 1
                               ELSE IF r \in released \cup removed THEN 0
                               ELSE tries[r]]
  \* A claimed root passes every root that became ready before it and still waits.
  /\ overtaken' = overtaken \cup
                    UNION {Before(readyOrder, r) \cap {q \in Roots : Eligible(q)} : r \in claimed}

\* A SyncOwnership step followed by the bookkeeping; the wall clock is idle.
W(A) == A /\ QueueStep /\ UNCHANGED expired

\* The clock claims a slot only for a due root, and only for the oldest due
\* root that could take one: a root in backoff is not due, so a root that
\* became due later can go before it.
WClaim(s, v) ==
  /\ <<s, v>> \in Members(dueOrder)
  /\ ClaimFetch(s, v)
  /\ \A q \in Before(dueOrder, <<s, v>>) : ~Eligible(q)
  /\ QueueStep
  /\ UNCHANGED expired

\* A failed attempt gives the slot back. Under WholeTask only the last of the
\* task's attempts does; the earlier ones retry in place (below). A trickling
\* provider delivers every window, so its attempts never stall.
WFail(s, v) ==
  /\ s \notin Trickle
  /\ FailFetchBeforeWaiter(s, v)
  /\ (AttemptMode = "PerAttempt" \/ tries[<<s, v>>] >= MaxAttempts)
  /\ QueueStep
  /\ UNCHANGED expired

\* A completed fetch, which a provider that never serves never gives, and a
\* trickling one never gives within the horizon of this model.
WComplete(s, v) ==
  /\ s \notin Dead \cup Trickle
  /\ W(CompleteFetch(s, v))

\* A trickling attempt's window delivers a block: progress, with nothing for
\* the clock to act on. Without a quantum, the attempt goes on like this.
TrickleWindow(s, v) ==
  /\ s \in Trickle
  /\ <<s, v>> \in pending
  /\ flights[<<s, v>>] > 0
  /\ UNCHANGED allVars

\* Quantum only: an attempt still making progress reaches its quantum
\* (CLOCK_ATTEMPT_QUANTUM). It gives its slot back at the next window
\* boundary, keeping what it fetched, and its root is due again at once,
\* behind every root already due (FetchTaskEnd::Preempted, finish_attempt).
\* It is not a failure: no backoff, no exhaustion.
Preempt(s, v) ==
  /\ CutoffMode = "Quantum"
  /\ s \in Trickle
  \* The slot is released like a deferral: no exhaustion, the record stays.
  /\ ContendedIngest(s, v)
  \* Due now, behind every root that was due before it.
  /\ dueOrder' = Append(Without(dueOrder, {<<s, v>>}), <<s, v>>)
  /\ readyOrder' = Append(Without(readyOrder, {<<s, v>>}), <<s, v>>)
  /\ backoff' = backoff \ {<<s, v>>}
  /\ tries' = [tries EXCEPT ![<<s, v>>] = 0]
  /\ UNCHANGED <<overtaken, expired>>

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
  \* It can run at once unless an attempt is still running.
  /\ readyOrder' = IF flights[<<s, v>>] = 0 THEN Append(readyOrder, <<s, v>>) ELSE readyOrder
  \* Nothing else changes.
  /\ UNCHANGED <<vars, tries, overtaken, expired>>

\* WholeTask only: a failed attempt keeps its slot, sleeps, and starts the next
\* attempt without asking the clock, ahead of every root that is waiting.
RetryInPlace(s, v) ==
  \* Only the old hold-the-slot rule retries inside the task.
  /\ AttemptMode = "WholeTask"
  \* The root is still owed and its fetch holds a slot.
  /\ <<s, v>> \in pending
  /\ flights[<<s, v>>] > 0
  \* The task has attempts left.
  /\ tries[<<s, v>>] < MaxAttempts
  \* The next attempt starts.
  /\ tries' = [tries EXCEPT ![<<s, v>>] = @ + 1]
  \* Every root waiting for a slot was ready before this attempt and is passed.
  /\ overtaken' = overtaken \cup {q \in Members(readyOrder) : Eligible(q)}
  \* Nothing else changes: the slot stays with this root.
  /\ UNCHANGED <<vars, dueOrder, readyOrder, backoff, expired>>

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
  \* Record the drop.
  /\ expired' = expired \cup {<<s, v>>}
  \* The sender's state and the rest of the receiver are untouched.
  /\ UNCHANGED <<localV, dirty, payloadLedger, inflight, serveAuth, serveScopes,
                 ready, mergeFlights, terminalFlights, fetchExhausted, mergedV,
                 crashed>>
  \* The queues drop it too.
  /\ QueueStep

\* The restart: the resync sweep restores every durable root due at once, in
\* the sweep's order, with no fetch and no backoff.
WCrash ==
  /\ Crash
  /\ dueOrder' = AsSeq(pending')
  /\ readyOrder' = AsSeq(pending')
  /\ backoff' = {}
  /\ tries' = [r \in Roots |-> 0]
  /\ UNCHANGED <<overtaken, expired>>

\* Nothing queued, no attempts, nothing passed, nothing dropped.
WInit ==
  /\ Init
  /\ dueOrder = <<>>
  /\ readyOrder = <<>>
  /\ backoff = {}
  /\ tries = [r \in Roots |-> 0]
  /\ overtaken = {}
  /\ expired = {}

\* Every SyncOwnership action, with the claim, the failure, the completion and
\* the restart replaced by their dispatch-aware forms, plus the deadline, the
\* trickling window, the quantum, the in-place retry and the TTL.
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
       \/ W(ContendedIngest(s, v)) \/ W(ProviderShed(s, v))
       \/ W(StarveTransportCompletion(s, v)) \/ W(StarveRecoveryServe(s, v))
       \/ W(StarveRecoveryAuthorization(s, v))
       \/ WComplete(s, v) \/ W(ClaimMerge(s, v))
       \/ W(CompleteMerge(s, v)) \/ W(ClaimTerminal(s, v))
       \/ W(CompleteTerminal(s, v))
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
\* The one environment assumption: a fetch that can complete eventually does
\* (strong fairness on some WComplete). Reachability alone does not give it:
\* a provider can answer busy, or stall, every time. Dead and Trickle
\* providers never complete here; their heads stay owned
\* (INV_ExpiryKeepsOwner), and LIVE_AckedRootMerged promises them nothing.
WFairSpec ==
  /\ WSpec
  /\ WF_allVars(\E s \in Scopes, v \in 1..MaxV : WClaim(s, v))
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
  /\ readyOrder \in Seq(Roots)
  /\ backoff \subseteq Roots
  /\ tries \in [Roots -> 0..MaxAttempts]
  /\ overtaken \subseteq Roots
  /\ expired \subseteq Roots

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

\* Promise: a due root waiting for a slot is never passed by one that became due after it.
INV_ReadyRootsServedInOrder == overtaken = {}

\* Promise: an acknowledged head never leaves the receiver while nobody else owns it.
INV_ExpiryKeepsOwner == \A r \in expired : Owned(r)

\* Promise: every acknowledged head whose provider serves is merged, whatever
\* never-serving or trickling heads hold the slots.
LIVE_AckedRootMerged ==
  \A r \in Roots : r[1] \notin Dead \cup Trickle => ((r \in pending) ~> (mergedV[r[1]] >= r[2]))
====

---- MODULE MC_SyncOwnership_Dispatch ----
\* The story. A receiver that acknowledged a head owes its sender a fetch of
\* that head's DAG: the success reply ended the sender's retry marker, so the
\* receiver is now the only one who remembers the head. The receiver's
\* per-root retry clock hands roots to a small pool of fetch slots (FetchCap).
\* Fetches can fail, the receiver can restart, and new heads keep arriving.
\*
\* Who acts: the sender (writes and hints), the receiver's clock (claims a
\* slot for the oldest due root), each fetch (one attempt, which completes or
\* fails), the merge writer, a one-shot restart, and the wall clock.
\*
\* What must never happen:
\*   - a root that is waiting for a slot is passed by a root that became ready
\*     after it (INV_ReadyRootsServedInOrder): otherwise, after a restart,
\*     catch-up roots retrying in place can hold every slot and starve each
\*     newly acknowledged head;
\*   - an acknowledged head leaves the receiver while nobody else still owns
\*     it (INV_ExpiryKeepsOwner): a wall-clock TTL would drop acknowledged
\*     heads whose senders have already forgotten them;
\* and what must eventually happen: every acknowledged head is fetched and
\* merged (LIVE_AckedRootMerged), while its provider stays reachable.
\*
\* This module adds the dispatch layer to SyncOwnership.tla without changing
\* it: every SyncOwnership step runs here too, followed by the bookkeeping
\* of the clock's queue.
\*
\* Code it models (crates/p2p/src/sync):
\*   coordinator/mod.rs  dispatch_due_pending_dag_fetches (oldest due first, one slot each)
\*   coordinator/dag_fetcher.rs  poll_fetch_dag (a clock-owned fetch makes one attempt)
\*   replication/handlers.rs  handle_dag_needs_fetch (the slot comes back; the clock re-arms the root)
\*   manager/process/pending_dag.rs  PendingDagLease::finish_attempt, due_pending_dag_retries
\*   manager/pending.rs  PendingDag::is_expired (only an unacknowledged entry expires)
\* Rust tests: tests/unit/restart_slot_fairness.rs, tests/unit/acked_pending_ttl.rs.
EXTENDS SyncOwnership, Sequences

CONSTANTS
  AttemptMode, \* "PerAttempt": a fetch gives its slot back after one attempt | "WholeTask": the bug, it keeps the slot through every attempt
  RearmMode,   \* "AttemptEnd": a root is due again from the end of its attempt | "ClaimTime": the bug, its next due time is fixed when it is claimed
  ExpiryMode,  \* "AckRetained": an acknowledged root never expires | "WallClockTtl": the bug, a TTL may drop it at any time
  MaxAttempts  \* attempts one fetch task makes before it gives up (MAX_FETCH_ATTEMPTS)

\* Each knob takes one of its named values.
ASSUME AttemptMode \in {"PerAttempt", "WholeTask"}
\* The re-arm knob takes one of its named values.
ASSUME RearmMode \in {"AttemptEnd", "ClaimTime"}
\* The expiry knob takes one of its named values.
ASSUME ExpiryMode \in {"AckRetained", "WallClockTtl"}
\* A fetch task makes at least one attempt.
ASSUME MaxAttempts \in Nat /\ MaxAttempts >= 1
\* One fetch owner per root: the claim and the queue below assume it.
ASSUME FlightMode = "SingleFlight"

VARIABLES
  dueOrder,   \* the clock's queue: pending roots in next_retry_at order
  readyOrder, \* pending roots without a slot, in the order they became able to run
  tries,      \* [Roots -> 0..MaxAttempts], attempts the current fetch of a root has started
  overtaken,  \* roots that waited for a slot while a root behind them started an attempt
  expired     \* acknowledged roots the wall clock dropped

\* The variables this module adds.
dvars == <<dueOrder, readyOrder, tries, overtaken, expired>>
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
  IN
  \* A root becomes able to run when acknowledged or when its fetch ends.
  /\ readyOrder' = Without(readyOrder, removed \cup claimed) \o AsSeq(added \cup released)
  \* AttemptEnd queues a root again when its attempt ends; ClaimTime queues it
  \* again the moment it is claimed, so it already sits ahead of every root
  \* acknowledged while its attempt ran.
  /\ dueOrder' = IF RearmMode = "AttemptEnd"
                 THEN Without(dueOrder, removed \cup claimed) \o AsSeq(added \cup released)
                 ELSE Without(dueOrder, removed \cup claimed) \o AsSeq(added \cup claimed)
  \* A claim starts attempt one; a released or retired root has no attempt.
  /\ tries' = [r \in Roots |-> IF r \in claimed THEN 1
                               ELSE IF r \in released \cup removed THEN 0
                               ELSE tries[r]]
  \* A claimed root passes every root that became ready before it and still waits.
  /\ overtaken' = overtaken \cup
                    UNION {Before(readyOrder, r) \cap {q \in Roots : Eligible(q)} : r \in claimed}

\* A SyncOwnership step followed by the bookkeeping; the wall clock is idle.
W(A) == A /\ QueueStep /\ UNCHANGED expired

\* The clock claims a slot only for the oldest due root that could take one.
WClaim(s, v) ==
  /\ ClaimFetch(s, v)
  /\ \A q \in Before(dueOrder, <<s, v>>) : ~Eligible(q)
  /\ QueueStep
  /\ UNCHANGED expired

\* A failed attempt gives the slot back. Under WholeTask only the last of the
\* task's attempts does; the earlier ones retry in place (below).
WFail(s, v) ==
  /\ FailFetchBeforeWaiter(s, v)
  /\ (AttemptMode = "PerAttempt" \/ tries[<<s, v>>] >= MaxAttempts)
  /\ QueueStep
  /\ UNCHANGED expired

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
  /\ UNCHANGED <<vars, dueOrder, readyOrder, expired>>

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

\* Nothing queued, no attempts, nothing passed, nothing dropped.
WInit ==
  /\ Init
  /\ dueOrder = <<>>
  /\ readyOrder = <<>>
  /\ tries = [r \in Roots |-> 0]
  /\ overtaken = {}
  /\ expired = {}

\* Every SyncOwnership action, with the claim and the failure replaced by
\* their dispatch-aware forms, plus the in-place retry and the TTL.
WNext ==
  \/ \E s \in Scopes : W(Update(s)) \/ W(ReHintMarker(s))
  \/ \E s \in Scopes, v \in 1..MaxV :
       \/ W(ReHintPayload(s, v)) \/ W(DropHint(s, v)) \/ W(ProcessMerged(s, v))
       \/ W(ProcessRegister(s, v)) \/ W(ProcessNack(s, v))
       \/ W(SameRootReannounce(s, v))
       \/ W(OfferQualifiedAlternate(s, v)) \/ W(AddQualifiedAlternate(s, v))
       \/ WClaim(s, v) \/ WFail(s, v) \/ RetryInPlace(s, v) \/ Expire(s, v)
       \/ W(ContendedIngest(s, v)) \/ W(ProviderShed(s, v))
       \/ W(StarveTransportCompletion(s, v)) \/ W(StarveRecoveryServe(s, v))
       \/ W(StarveRecoveryAuthorization(s, v))
       \/ W(CompleteFetch(s, v)) \/ W(ClaimMerge(s, v))
       \/ W(CompleteMerge(s, v)) \/ W(ClaimTerminal(s, v))
       \/ W(CompleteTerminal(s, v))
  \/ W(Crash)
  \/ W(Terminating)

\* Safety runs.
WSpec == WInit /\ [][WNext]_allVars

\* Liveness runs: once a head is acknowledged only the receiver acts on it, so
\* only the receiver is fair: the clock claims, and every root's fetch, merge
\* and cleanup eventually run when they keep being possible. Sender fairness
\* is SyncOwnership's own liveness configs. Failures, retries in place, the
\* TTL and the restart are never forced.
WFairSpec ==
  WSpec /\ \A s \in Scopes :
    /\ SF_allVars(\E v \in 1..MaxV : WClaim(s, v))
    /\ SF_allVars(\E v \in 1..MaxV : W(CompleteFetch(s, v)))
    /\ SF_allVars(\E v \in 1..MaxV : W(ClaimMerge(s, v)))
    /\ SF_allVars(\E v \in 1..MaxV : W(CompleteMerge(s, v)))

\* Promise: the added variables keep their shapes, on top of SyncOwnership's.
INV_DispatchTypeOK ==
  /\ TypeOK
  /\ dueOrder \in Seq(Roots)
  /\ readyOrder \in Seq(Roots)
  /\ tries \in [Roots -> 0..MaxAttempts]
  /\ overtaken \subseteq Roots
  /\ expired \subseteq Roots

\* Someone still owns head r: it merged, the sender keeps its scope marker, a
\* hint for it or a newer head is in flight, or the receiver owes it or a newer head.
Owned(r) ==
  \/ mergedV[r[1]] >= r[2]
  \/ r[1] \in dirty
  \/ \E u \in inflight \cup pending : u[1] = r[1] /\ u[2] >= r[2]

\* Promise: a root waiting for a slot is never passed by one that became ready after it.
INV_ReadyRootsServedInOrder == overtaken = {}

\* Promise: an acknowledged head never leaves the receiver while nobody else owns it.
INV_ExpiryKeepsOwner == \A r \in expired : Owned(r)

\* Promise: every acknowledged head is eventually merged.
LIVE_AckedRootMerged == \A r \in Roots : (r \in pending) ~> (mergedV[r[1]] >= r[2])
====

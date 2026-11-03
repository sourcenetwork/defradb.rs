---- MODULE PushLogAdmission ----
\* PushLog admission at the hub's sync manager (#1088 slice 1 / W1, and F1 sibling
\* heads), abstracting crates/p2p/src/sync/manager/process/pushlog.rs
\* (process_block_inner, assert_pushlog_left_durable_state), the scope rule in
\* crates/p2p/src/sync/head_scope.rs (ScopeDecision::of), the in-memory registry in
\* crates/p2p/src/sync/manager/process/pending_dag.rs (try_insert_pending_dag), the
\* durable one in crates/p2p/src/sync/pending_store/store.rs (register), and the
\* reply seams in crates/p2p/src/sync/coordinator/event_handler/pushlog.rs.
\*
\* THE STORY: a pusher sends the hub one head block at a time. The hub either merges
\* the head, registers it as pending (it owns fetching the rest of its DAG), or nacks
\* it. A success reply makes the pusher delete its retry record, so after a success
\* the hub alone owns the head. Heads form a DAG: a head links its parents, and
\* fetching a head fetches every ancestor. Two heads of one document written on two
\* different peers are siblings: neither links the other.
\*
\* THE MECHANISM: the hub remembers one current head per sender scope (the announcing
\* peer, the collection and the document). A new head of that scope may retire the
\* current one (it "supersedes" it), and the current head may make a new head need no
\* registration at all (it "covers" it).
\*
\* THE BUGS:
\*   - #1088 M1 (ReplyMode = "SuccessOnFull"): at capacity the hub drops the
\*     registration but replies success. The pusher deletes its retry record and
\*     the hub holds nothing.
\*   - F1 (CoverRule = "Priority"): the hub decided cover and supersede by
\*     (priority, CID), as if a newer head always carried the older one. A sibling
\*     with a lower version was acked as covered and never registered; one with a
\*     higher version retired the earlier sibling's registration. Neither sibling
\*     links the other, so fetching the survivor never fetches the lost one.
\*
\* THE INVARIANT: every head the pusher was told succeeded is reachable from what the
\* hub holds: a pending head or a merged one, or an ancestor of one.
\*
\* Two knobs:
\*   ReplyMode = "NackOnFull"    - capacity overflow nacks; the pusher retries. [GREEN]
\*             = "SuccessOnFull" - overflow drops the head and acks success. [RED]
\*   CoverRule = "Parent"        - a head retires the current head only when it links
\*                                 it as a parent, and nothing is covered: every
\*                                 other head registers on its own. [GREEN, the code]
\*             = "Priority"      - (priority, CID) order covers and supersedes. [RED]
\*
\* Abstractions: the pusher is one per-head ack/retry record; heads of different
\* scopes are independent. HOW a push comes to have missing links is abstracted:
\* HubComplete models an arrival whose DAG is already complete, HubAdmit/HubOverflow*
\* one that is not. Merging a head merges its whole DAG, so its ancestors' pending
\* records go with it (in the code the clock or the resync sweep finds them merged
\* and discharges them). TTL eviction is not modeled (see PendingDagRestart).
EXTENDS Naturals, FiniteSets

CONSTANTS
  Heads,       \* the pushed heads, as numbers: their order is the CID tie-break
  Parent,      \* [Heads -> SUBSET Heads]: the heads each head links as parents
  Prio,        \* [Heads -> Nat]: each head's priority
  ScopeOf,     \* [Heads -> scopes]: each head's sender scope
  Cap,         \* admission capacity: SyncConfig::max_pending_dags with no durable store; with one, the
               \* durable tier (a full map admits on the record alone, MC_SyncOwnership_Memory)
  ReplyMode,   \* "NackOnFull" | "SuccessOnFull"
  CoverRule    \* "Parent" | "Priority"

\* NoHead stands for "the scope has no current head"; it is never a real head.
NoHead == 0

ASSUME Heads # {} /\ Heads \subseteq Nat /\ NoHead \notin Heads
ASSUME Parent \in [Heads -> SUBSET Heads]
ASSUME Prio \in [Heads -> Nat]
ASSUME Cap \in Nat /\ Cap >= 1
ASSUME ReplyMode \in {"NackOnFull", "SuccessOnFull"}
ASSUME CoverRule \in {"Parent", "Priority"}

\* The sender scopes that any head belongs to.
Scopes == {ScopeOf[h] : h \in Heads}

\* Every ancestor of h: its parents, their parents, and so on.
RECURSIVE AncOf(_)
AncOf(h) == Parent[h] \cup UNION {AncOf(p) : p \in Parent[h]}

\* A parent always has a lower priority than its child, and no head is its own
\* ancestor: the parent relation is a DAG.
ASSUME \A h \in Heads : \A p \in Parent[h] : Prio[p] < Prio[h]

\* The heads S holds, with every ancestor of them: what fetching S fetches.
Reach(S) == S \cup UNION {AncOf(h) : h \in S}

\* The code's version order, (priority, CID): a before b.
Before(a, b) == Prio[a] < Prio[b] \/ (Prio[a] = Prio[b] /\ a < b)

\* Pusher-side per-head record:
\*   unsent       - not yet pushed
\*   inflight     - push sent, awaiting PushLogReply
\*   ackedSuccess - success reply consumed; retry record DELETED (terminal on the pusher)
\*   retryQueued  - error reply consumed; retry record persisted, will re-push
PusherStates == {"unsent", "inflight", "ackedSuccess", "retryQueued"}

VARIABLES
  pending,   \* SUBSET Heads - hub pending-DAG registrations awaiting missing links
  merged,    \* SUBSET Heads - merged on the hub
  cur,       \* [Scopes -> Heads \cup {NoHead}] - each scope's current head
  pusher     \* [Heads -> PusherStates]

vars == <<pending, merged, cur, pusher>>

TypeOK ==
  /\ pending \subseteq Heads
  /\ merged \subseteq Heads
  /\ pending \cap merged = {}
  /\ Cardinality(pending) <= Cap
  /\ cur \in [Scopes -> Heads \cup {NoHead}]
  /\ pusher \in [Heads -> PusherStates]

Init ==
  /\ pending = {}
  /\ merged = {}
  /\ cur = [s \in Scopes |-> NoHead]
  /\ pusher = [h \in Heads |-> "unsent"]

\* The current head of h's scope (ScopeDecision::of reads it).
CurOf(h) == cur[ScopeOf[h]]

\* Whether the current head makes h need no registration (ScopeDecision
\* CoveredByCurrent). Under "Priority" a lower version is covered whether or not the
\* current head links it; under "Parent" nothing is covered.
Covered(h) ==
  /\ CoverRule = "Priority"
  \* only the old rule covers anything;
  /\ CurOf(h) # NoHead
  \* the scope has a current head;
  /\ CurOf(h) # h
  \* that is another head;
  /\ Before(h, CurOf(h))
  \* and h has the lower version, linked or not.

\* The current head h retires, or NoHead (ScopeDecision Supersedes). Under "Parent"
\* only a current head that h links as a parent; under "Priority" any lower version.
Retires(h) ==
  LET c == CurOf(h) IN
  \* c is the scope's current head;
  IF c = NoHead \/ c = h THEN NoHead
  \* no current head, or h itself: nothing to retire;
  ELSE IF CoverRule = "Parent" THEN (IF c \in Parent[h] THEN c ELSE NoHead)
  \* the code: retire c only when h links it, so fetching h fetches c;
  ELSE IF Before(c, h) THEN c ELSE NoHead
  \* the old rule: retire any lower version.

\* The scope's current head after h registers: h, unless the scope keeps a newer
\* current head (a sibling registered on its own leaves a newer one in place).
NextCur(h) ==
  LET c == CurOf(h) IN
  \* c is the scope's current head before h registers;
  IF c = NoHead \/ Retires(h) # NoHead \/ ~Before(h, c) THEN h ELSE c
  \* h takes over when there is none, when h retires it, or when h is newer.

\* Scopes whose current head is in gone lose it.
ClearCur(gone) == [s \in Scopes |-> IF cur[s] \in gone THEN NoHead ELSE cur[s]]

\* Pusher sends (or re-sends from its persisted retry ladder) the head's push.
Send(h) ==
  /\ pusher[h] \in {"unsent", "retryQueued"}
  /\ pusher' = [pusher EXCEPT ![h] = "inflight"]
  /\ UNCHANGED <<pending, merged, cur>>

\* A covered head is acked with no state change (pushlog.rs: the in-memory and
\* durable CoveredByCurrent paths return Ok without registering).
HubCovered(h) ==
  /\ pusher[h] = "inflight"
  \* the push is on the wire;
  /\ h \notin merged
  \* the hub has not merged it;
  /\ h \notin pending
  \* nor registered it;
  /\ Covered(h)
  \* but the current head covers it;
  /\ pusher' = [pusher EXCEPT ![h] = "ackedSuccess"]
  \* so the hub acks it, and the pusher forgets it;
  /\ UNCHANGED <<pending, merged, cur>>
  \* while the hub records nothing.

\* A head the hub already registered is acked again (extend_registered_root).
HubReack(h) ==
  /\ pusher[h] = "inflight"
  \* the push is on the wire;
  /\ h \in pending
  \* the hub already owns h;
  /\ pusher' = [pusher EXCEPT ![h] = "ackedSuccess"]
  \* so it acks it;
  /\ UNCHANGED <<pending, merged, cur>>
  \* and nothing else changes.

\* The pushed DAG is complete on arrival, or already merged: the hub merges it, and
\* so its ancestors, and replies success. Success here is honest.
HubComplete(h) ==
  /\ pusher[h] = "inflight"
  \* the push is on the wire;
  /\ ~Covered(h)
  \* a covered head is answered before anything is merged;
  /\ merged' = merged \cup Reach({h})
  \* h and its whole DAG merge;
  /\ pending' = pending \ Reach({h})
  \* records of anything that merged are discharged;
  /\ cur' = ClearCur(Reach({h}))
  \* scopes whose current head merged have none;
  /\ pusher' = [pusher EXCEPT ![h] = "ackedSuccess"]
  \* and the hub acks h.

\* Missing links and a free slot, or a current head h retires (the retiree's slot
\* passes to h): register pending, become the scope's current head unless a newer
\* one is kept, reply success.
HubAdmit(h) ==
  /\ pusher[h] = "inflight"
  \* the push is on the wire;
  /\ h \notin merged
  \* the hub has not merged h;
  /\ h \notin pending
  \* nor registered it;
  /\ ~Covered(h)
  \* nor treats it as covered;
  /\ (Cardinality(pending) < Cap \/ Retires(h) # NoHead)
  \* a slot is free, or h takes the slot of the head it retires;
  /\ pending' = (pending \ {Retires(h)}) \cup {h}
  \* the retired head's record goes and h's arrives;
  /\ cur' = [cur EXCEPT ![ScopeOf[h]] = NextCur(h)]
  \* the scope's current head moves on;
  /\ pusher' = [pusher EXCEPT ![h] = "ackedSuccess"]
  \* the hub acks h;
  /\ UNCHANGED merged
  \* and nothing merges yet.

\* The head would need a new slot and none is free.
Overflows(h) ==
  /\ pusher[h] = "inflight"
  \* the push is on the wire;
  /\ h \notin merged
  \* the hub has not merged h;
  /\ h \notin pending
  \* nor registered it;
  /\ ~Covered(h)
  \* nor treats it as covered;
  /\ Retires(h) = NoHead
  \* h frees no slot by retiring a head;
  /\ Cardinality(pending) >= Cap
  \* and every slot is taken.

\* Capacity overflow, historical regression: the registration is DROPPED but the
\* pusher is told success, so it deletes its retry record. [RED path]
HubOverflowSuccess(h) ==
  /\ ReplyMode = "SuccessOnFull"
  /\ Overflows(h)
  /\ pusher' = [pusher EXCEPT ![h] = "ackedSuccess"]
  /\ UNCHANGED <<pending, merged, cur>>

\* Capacity overflow, fixed behavior: nack (AT_CAPACITY). The pusher keeps its retry
\* record and re-pushes. [GREEN path]
HubOverflowNack(h) ==
  /\ ReplyMode = "NackOnFull"
  /\ Overflows(h)
  /\ pusher' = [pusher EXCEPT ![h] = "retryQueued"]
  /\ UNCHANGED <<pending, merged, cur>>

\* The fetch completes a registered DAG and it merges, with every ancestor
\* (retry_pending_dag -> DagReady -> merge); records of merged ancestors go too.
Resolve(h) ==
  /\ h \in pending
  \* the hub owns h;
  /\ merged' = merged \cup Reach({h})
  \* h and its whole DAG merge;
  /\ pending' = pending \ Reach({h})
  \* records of anything that merged are discharged;
  /\ cur' = ClearCur(Reach({h}))
  \* scopes whose current head merged have none;
  /\ UNCHANGED pusher
  \* and the pusher is not involved.

\* Every reply the hub can give to the push of h.
HubReply(h) ==
  \/ HubCovered(h)
  \/ HubReack(h)
  \/ HubComplete(h)
  \/ HubAdmit(h)
  \/ HubOverflowSuccess(h)
  \/ HubOverflowNack(h)

Next ==
  \E h \in Heads :
    \/ Send(h)
    \/ HubReply(h)
    \/ Resolve(h)

\* All heads merged: stutter so TLC does not flag deadlock on a finished schedule.
Done == merged = Heads
Terminating == Done /\ UNCHANGED vars

Spec == Init /\ [][Next \/ Terminating]_vars

\* Scheduler-level fairness only: the pusher keeps sending something, the hub keeps
\* answering something, the fetch pool keeps completing something. No head is promised
\* a turn of its own; per-head progress has to follow from these.
FairSpec ==
  /\ Spec
  /\ WF_vars(\E h \in Heads : Send(h))
  /\ WF_vars(\E h \in Heads : HubReply(h))
  /\ WF_vars(\E h \in Heads : Resolve(h))

INV_TypeOK == TypeOK

\* THE #1088 M1 INVARIANT: a success reply implies the hub kept the very head: it is
\* merged or registered as pending. It holds only when no head retires another, so it
\* is checked on configurations whose heads have no parents.
INV_SuccessImpliesRegisteredOrMerged ==
  \A h \in Heads : pusher[h] = "ackedSuccess" => h \in pending \cup merged

\* THE F1 INVARIANT: a success reply implies the hub can still reach the head: it is
\* pending or merged, or an ancestor of a head that is. A retired parent passes; a
\* covered or retired sibling does not.
INV_AckedHeadReachable ==
  \A h \in Heads : pusher[h] = "ackedSuccess" => h \in Reach(pending \cup merged)

\* Liveness (GREEN, under FairSpec): with overflow nacked and every acked head
\* reachable, every pushed head eventually merges.
EventuallyAllMerged == <>(merged = Heads)
====

---- MODULE PushLogAdmission ----
\* PushLog admission at the hub's sync manager (#1088 slice 1 / W1, and F1 sibling
\* heads), abstracting crates/p2p/src/sync/manager/process/pushlog.rs
\* (process_block_inner, assert_pushlog_left_durable_state), the scope rule in
\* crates/p2p/src/sync/head_scope.rs (ScopeDecision::of), the in-memory registry in
\* crates/p2p/src/sync/manager/process/pending_dag.rs (try_insert_pending_dag), the
\* durable one in crates/p2p/src/sync/pending_store/store.rs (register), the hub's view
\* of it in crates/p2p/src/sync/pending_store/view.rs (PersistedView), the Processed
\* outcome of process_block_inner, and the reply seams in
\* crates/p2p/src/sync/coordinator/event_handler/pushlog.rs.
\* Rust test: crates/p2p/tests/unit/durable_only_ack.rs.
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
\* THE ACK GUARD (assert_pushlog_left_durable_state): before a success reply the hub checks
\* that the push left the head merged or registered. A head the full in-memory map has no
\* entry for is registered on its durable record alone (process_block_inner, resident false),
\* and the hub's view of its durable records (PersistedView, pending_store/view.rs) learns of
\* a commit only when the store's receipt-ordered events go out (OnCommit::Ordered,
\* pending_store/store.rs on_commit). An earlier commit's hook on the same store, such as a
\* merge's, holds those events back past the reply. A guard that reads only the view then
\* nacks a head whose record is durable; the reply seams map that nack to RATE_LIMITED, and
\* the sender parks the hub for it.
\*
\* THE INVARIANT: every head the pusher was told succeeded is reachable from what the
\* hub holds: a pending head or a merged one, or an ancestor of one.
\*
\* Three knobs:
\*   ReplyMode = "NackOnFull"    - capacity overflow nacks; the pusher retries. [GREEN]
\*             = "SuccessOnFull" - overflow drops the head and acks success. [RED]
\*   CoverRule = "Parent"        - a head retires the current head only when it links
\*                                 it as a parent, and nothing is covered: every
\*                                 other head registers on its own. [GREEN, the code]
\*             = "Priority"      - (priority, CID) order covers and supersedes. [RED]
\*   GuardMode = "Committed"     - the guard accepts the durable record the store committed
\*                                 for this push (Processed::DurablyRegistered). [GREEN]
\*             = "View"          - the guard reads only the view, which may not show that
\*                                 record yet: a head the hub owns is nacked. [RED]
\*
\* Abstractions: the pusher is one per-head ack/retry record; heads of different
\* scopes are independent. HOW a push comes to have missing links is abstracted:
\* HubComplete models an arrival whose DAG is already complete, HubAdmit/HubOverflow*
\* one that is not. Merging a head merges its whole DAG, so its ancestors' pending
\* records go with it (in the code the clock or the resync sweep finds them merged
\* and discharges them). TTL eviction is not modeled (see PendingDagRestart). The view
\* learns of a removal at once here: a merged head is acked by its merge, and a retired
\* one by the head that links it, so the lag of a removal never decides a reply.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Heads,       \* the pushed heads, as numbers: their order is the CID tie-break
  Parent,      \* [Heads -> SUBSET Heads]: the heads each head links as parents
  Prio,        \* [Heads -> Nat]: each head's priority
  ScopeOf,     \* [Heads -> scopes]: each head's sender scope
  Cap,         \* admission capacity: SyncConfig::max_pending_dags with no durable store; with one, the
               \* durable tier (a full map admits on the record alone, MC_SyncOwnership_Memory)
  ReplyMode,   \* "NackOnFull" | "SuccessOnFull"
  CoverRule,   \* "Parent" | "Priority"
  GuardMode    \* "Committed" | "View"

\* NoHead stands for "the scope has no current head"; it is never a real head.
NoHead == 0

ASSUME Heads # {} /\ Heads \subseteq Nat /\ NoHead \notin Heads
ASSUME Parent \in [Heads -> SUBSET Heads]
ASSUME Prio \in [Heads -> Nat]
ASSUME Cap \in Nat /\ Cap >= 1
ASSUME ReplyMode \in {"NackOnFull", "SuccessOnFull"}
ASSUME CoverRule \in {"Parent", "Priority"}
\* The guard knob takes one of its named values.
ASSUME GuardMode \in {"Committed", "View"}

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
  pusher,    \* [Heads -> PusherStates]
  view       \* SUBSET Heads - the registrations the hub's view of its durable records shows

vars == <<pending, merged, cur, pusher, view>>

TypeOK ==
  /\ pending \subseteq Heads
  /\ merged \subseteq Heads
  /\ pending \cap merged = {}
  /\ Cardinality(pending) <= Cap
  /\ cur \in [Scopes -> Heads \cup {NoHead}]
  /\ pusher \in [Heads -> PusherStates]
  \* The view shows only registrations the store holds.
  /\ view \subseteq pending

Init ==
  /\ pending = {}
  /\ merged = {}
  /\ cur = [s \in Scopes |-> NoHead]
  /\ pusher = [h \in Heads |-> "unsent"]
  \* The view shows nothing yet.
  /\ view = {}

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
  /\ UNCHANGED <<pending, merged, cur, view>>

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
  /\ UNCHANGED <<pending, merged, cur, view>>
  \* while the hub records nothing.

\* A head the hub already registered is acked again (extend_registered_root).
HubReack(h) ==
  /\ pusher[h] = "inflight"
  \* the push is on the wire;
  /\ h \in pending
  \* the hub already owns h;
  /\ pusher' = [pusher EXCEPT ![h] = "ackedSuccess"]
  \* so it acks it;
  /\ UNCHANGED <<pending, merged, cur, view>>
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
  /\ view' = view \ Reach({h})
  \* the view no longer shows a registration the merge discharged;
  /\ pusher' = [pusher EXCEPT ![h] = "ackedSuccess"]
  \* and the hub acks h.

\* Where h may register: a slot is free, or h takes the slot of the head it retires.
\* Both tiers share this rule; which tier holds h decides only what the guard finds.
Admissible(h) ==
  /\ pusher[h] = "inflight"
  \* the push is on the wire;
  /\ h \notin merged
  \* the hub has not merged h;
  /\ h \notin pending
  \* nor registered it;
  /\ ~Covered(h)
  \* nor treats it as covered;
  /\ (Cardinality(pending) < Cap \/ Retires(h) # NoHead)
  \* and a slot is free, or h retires a head and takes its slot.

\* The registration of h: the retired head's record goes, h's arrives, and the scope's
\* current head moves on. The view drops the retired record; it learns of h's later
\* (Publish).
Register(h) ==
  /\ pending' = (pending \ {Retires(h)}) \cup {h}
  \* the retired head's record goes and h's arrives;
  /\ cur' = [cur EXCEPT ![ScopeOf[h]] = NextCur(h)]
  \* the scope's current head moves on;
  /\ view' = view \ {Retires(h)}
  \* and the view no longer shows the retired record.

\* Missing links and a free slot, or a current head h retires (the retiree's slot
\* passes to h): register pending with an in-memory entry, which the guard finds,
\* become the scope's current head unless a newer one is kept, reply success.
HubAdmit(h) ==
  /\ Admissible(h)
  \* h may register;
  /\ Register(h)
  \* h registers;
  /\ pusher' = [pusher EXCEPT ![h] = "ackedSuccess"]
  \* the hub acks h;
  /\ UNCHANGED merged
  \* and nothing merges yet.

\* Missing links, a free durable slot and a full in-memory map: h is registered on its
\* durable record alone (process_block_inner, resident false), so the guard finds no
\* entry. Committed: the guard accepts the record the store committed for this push.
\* View: the guard reads only the view, and a view that does not show the record yet
\* makes it nack a head whose record is durable; the pusher keeps its retry record.
HubAdmitDurableOnly(h) ==
  /\ Admissible(h)
  \* h may register;
  /\ Register(h)
  \* the store commits its record;
  /\ pusher' = [pusher EXCEPT ![h] =
                  IF GuardMode = "Committed" \/ h \in view
                  THEN "ackedSuccess"
                  ELSE "retryQueued"]
  \* the guard's reply: success, unless it reads a view that does not show the record;
  /\ UNCHANGED merged
  \* and nothing merges yet.

\* The store's receipt-ordered events go out (storage/backends/regolith/event_order.rs):
\* the view learns of a registration (PersistedView::registered).
Publish(h) ==
  /\ h \in pending
  \* a registration
  /\ h \notin view
  \* the view does not show yet;
  /\ view' = view \cup {h}
  \* shows now;
  /\ UNCHANGED <<pending, merged, cur, pusher>>
  \* and nothing else changes.

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
  /\ UNCHANGED <<pending, merged, cur, view>>

\* Capacity overflow, fixed behavior: nack (AT_CAPACITY). The pusher keeps its retry
\* record and re-pushes. [GREEN path]
HubOverflowNack(h) ==
  /\ ReplyMode = "NackOnFull"
  /\ Overflows(h)
  /\ pusher' = [pusher EXCEPT ![h] = "retryQueued"]
  /\ UNCHANGED <<pending, merged, cur, view>>

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
  /\ view' = view \ Reach({h})
  \* the view no longer shows a registration the merge discharged;
  /\ UNCHANGED pusher
  \* and the pusher is not involved.

\* Every reply the hub can give to the push of h.
HubReply(h) ==
  \/ HubCovered(h)
  \/ HubReack(h)
  \/ HubComplete(h)
  \/ HubAdmit(h)
  \/ HubAdmitDurableOnly(h)
  \/ HubOverflowSuccess(h)
  \/ HubOverflowNack(h)

Next ==
  \E h \in Heads :
    \/ Send(h)
    \/ HubReply(h)
    \/ Publish(h)
    \/ Resolve(h)

\* All heads merged: stutter so TLC does not flag deadlock on a finished schedule.
Done == merged = Heads
Terminating == Done /\ UNCHANGED vars

Spec == Init /\ [][Next \/ Terminating]_vars

\* Scheduler-level fairness only: the pusher keeps sending something, the hub keeps
\* answering something, the fetch pool keeps completing something, and the store's
\* events keep going out (a hook that holds them ends). No head is promised a turn of
\* its own; per-head progress has to follow from these.
FairSpec ==
  /\ Spec
  /\ WF_vars(\E h \in Heads : Send(h))
  /\ WF_vars(\E h \in Heads : HubReply(h))
  /\ WF_vars(\E h \in Heads : Resolve(h))
  /\ WF_vars(\E h \in Heads : Publish(h))

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

\* A nack implies the hub holds no registration for the head. The sender parks the hub on
\* a nack (RATE_LIMITED) and demotes its queued pushes, so a nack of a head the hub
\* already owns costs the sender for nothing. View violates this: the guard nacks a head
\* registered on its durable record alone before the view shows that record.
INV_NackImpliesUnregistered ==
  \A h \in Heads : pusher[h] = "retryQueued" => h \notin pending

\* Liveness (GREEN, under FairSpec): with overflow nacked and every acked head
\* reachable, every pushed head eventually merges.
EventuallyAllMerged == <>(merged = Heads)
====

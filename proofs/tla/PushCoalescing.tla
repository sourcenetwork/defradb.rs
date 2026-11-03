---- MODULE PushCoalescing ----
\* Latest-head retirement for outbound PushLog work (#1102), and the live
\* acknowledgement that completes a scope's durable marker (F1 sibling heads).
\*
\* THE STORY: a node produces heads of one document and offers each to a peer. Each
\* offer registers the head under the scope's durable marker (a presence marker with a
\* generation; Rust stores no head list) and queues a live push. The queue keeps only the
\* newest head per (scope, peer). A worker sends it; an ack delivers it. The retry
\* ladder, when the marker is set, sends every current head of the document and clears
\* the marker. Heads form a DAG: a head links its parents. Two heads merged from two
\* different writers are siblings: neither links the other.
\*
\* WHAT CAN GO WRONG:
\*   - head 1 becomes active, head 2 arrives and retires every queued/persisted
\*     predecessor, then head 1 fails: the failure must not recreate a persisted retry
\*     for head 1 (#1102);
\*   - a live ack clears the marker, which stands for every head registered under it.
\*     If the acked head does not reach a sibling registered before it (retired from the
\*     queue, or fenced out of the live path by the newer generation), that sibling is
\*     never sent again (F1).
\*
\* `persisted` is a ghost witness for the head whose failure dirtied a scope; `marker` is
\* a ghost witness for the heads the durable marker stands for; `offered` remembers every
\* head offered to a peer. Rust stores only the presence marker and its generation, and
\* rederives the current heads on retry.
\*
\* Code: crates/p2p/src/sync/push_backlog.rs (enqueue, job_done),
\* crates/p2p/src/sync/coordinator/push_worker.rs (run_push_job, is_whole_frontier,
\* report_push_ack), crates/p2p/src/sync/coordinator/broadcast.rs (admit_replicator_push),
\* crates/storage/src/stores/peerstore.rs (complete_retry_scope), and the ladder in
\* crates/db/src/merge/push_docs.rs (retry_doc, complete_document_retry_if_current).
EXTENDS Naturals, FiniteSets

CONSTANTS
  Peers,        \* the peers heads are offered to
  MaxHead,      \* heads 1..MaxHead are produced in this order
  Parent,       \* [1..MaxHead -> SUBSET (1..MaxHead)]: the heads each head links
  QueueMode,    \* "LatestOnly" | "AppendEvery"
  RetryMode,    \* "CurrentOnly" | "ReplayEvery"
  CompleteRule  \* "Frontier" | "Generation"

ASSUME Peers # {}
ASSUME MaxHead \in Nat /\ MaxHead >= 2
ASSUME QueueMode \in {"LatestOnly", "AppendEvery"}
ASSUME RetryMode \in {"CurrentOnly", "ReplayEvery"}
ASSUME CompleteRule \in {"Frontier", "Generation"}
\* A head links only heads produced before it, so the parents form a DAG.
ASSUME \A h \in 1..MaxHead : Parent[h] \subseteq 1..(h - 1)

VARIABLES nextHead, latest, queued, active, persisted, delivered, retired, marker, offered

vars == <<nextHead, latest, queued, active, persisted, delivered, retired, marker, offered>>
Heads == 1..MaxHead

\* Every ancestor of h: its parents, their parents, and so on.
RECURSIVE AncOf(_)
AncOf(h) == Parent[h] \cup UNION {AncOf(p) : p \in Parent[h]}

\* The heads S holds, with every ancestor: what a peer holding S can reach.
Reach(S) == S \cup UNION {AncOf(h) : h \in S}

\* The heads produced so far.
Produced == 1..(nextHead - 1)

\* The document's current heads: produced heads no other produced head links.
Frontier == {h \in Produced : \A g \in Produced : h \notin AncOf(g)}

TypeOK ==
  /\ nextHead \in 1..(MaxHead + 1)
  /\ latest \in [Peers -> 0..MaxHead]
  /\ queued \in [Peers -> SUBSET Heads]
  /\ active \in [Peers -> SUBSET Heads]
  /\ persisted \in [Peers -> SUBSET Heads]
  /\ delivered \in [Peers -> SUBSET Heads]
  /\ retired \in 0..(MaxHead * Cardinality(Peers) * 3)
  /\ marker \in [Peers -> SUBSET Heads]
  /\ offered \in [Peers -> SUBSET Heads]

Init ==
  /\ nextHead = 1
  /\ latest = [p \in Peers |-> 0]
  /\ queued = [p \in Peers |-> {}]
  /\ active = [p \in Peers |-> {}]
  /\ persisted = [p \in Peers |-> {}]
  /\ delivered = [p \in Peers |-> {}]
  /\ retired = 0
  /\ marker = [p \in Peers |-> {}]
  /\ offered = [p \in Peers |-> {}]

\* A newer local head is offered to one peer. Its registration sets the marker (and
\* takes the newest generation) before it is queued. LatestOnly atomically retires queued
\* and persisted predecessors before installing the new obligation.
Enqueue(p) ==
  /\ nextHead <= MaxHead
  /\ LET h == nextHead
         old == Cardinality(queued[p]) + Cardinality(persisted[p])
     IN /\ latest' = [latest EXCEPT ![p] = h]
        /\ queued' = [queued EXCEPT ![p] =
             IF QueueMode = "LatestOnly" THEN {h} ELSE @ \cup {h}]
        /\ persisted' = [persisted EXCEPT ![p] =
             IF QueueMode = "LatestOnly" THEN {} ELSE @]
        /\ retired' = retired + IF QueueMode = "LatestOnly" THEN old ELSE 0
        /\ marker' = [marker EXCEPT ![p] = @ \cup {h}]
        \* the marker now also stands for h;
        /\ offered' = [offered EXCEPT ![p] = @ \cup {h}]
        \* and h is offered to p for good;
  /\ nextHead' = nextHead + 1
  /\ UNCHANGED <<active, delivered>>

\* A worker takes a queued head and sends it.
Start(p, h) ==
  /\ h \in queued[p]
  /\ queued' = [queued EXCEPT ![p] = @ \ {h}]
  /\ active' = [active EXCEPT ![p] = @ \cup {h}]
  /\ UNCHANGED <<nextHead, latest, persisted, delivered, retired, marker, offered>>

\* A superseded active send may finish failing after a newer enqueue.  The
\* CurrentOnly path retires it; ReplayEvery recreates the stale persisted head.
Fail(p, h) ==
  /\ h \in active[p]
  /\ active' = [active EXCEPT ![p] = @ \ {h}]
  /\ persisted' = [persisted EXCEPT ![p] =
       IF RetryMode = "CurrentOnly" /\ h = latest[p] THEN {h} ELSE
       IF RetryMode = "ReplayEvery" THEN @ \cup {h} ELSE @]
  /\ retired' = retired + IF RetryMode = "CurrentOnly" /\ h < latest[p] THEN 1 ELSE 0
  /\ UNCHANGED <<nextHead, latest, queued, delivered, marker, offered>>

\* The persisted failure witness is sent again.
Retry(p, h) ==
  /\ h \in persisted[p]
  /\ persisted' = [persisted EXCEPT ![p] = @ \ {h}]
  /\ active' = [active EXCEPT ![p] = @ \cup {h}]
  /\ UNCHANGED <<nextHead, latest, queued, delivered, retired, marker, offered>>

\* Whether the live ack of h may clear its scope's marker. Generation: the ack of the
\* head holding the newest generation clears it (complete_retry_scope alone). Frontier:
\* only when h is also the document's whole frontier (is_whole_frontier), so every head
\* registered before it is one of its ancestors.
ClearsMarker(h) ==
  \/ CompleteRule = "Generation"
  \* the old rule: the newest generation's ack is enough;
  \/ Frontier \subseteq {h}
  \* the code: h must be the whole frontier.

\* The peer acks an active head: it is delivered. Only the head holding the newest
\* generation (the latest registration) can clear the marker, and only if ClearsMarker.
Ack(p, h) ==
  /\ h \in active[p]
  \* h was sent to p;
  /\ active' = [active EXCEPT ![p] = @ \ {h}]
  \* the send is over;
  /\ delivered' = [delivered EXCEPT ![p] = @ \cup {h}]
  \* p holds h;
  /\ marker' = [marker EXCEPT ![p] =
       IF h = latest[p] /\ ClearsMarker(h) THEN {} ELSE @]
  \* the marker clears only for the newest generation, under ClearsMarker;
  /\ UNCHANGED <<nextHead, latest, queued, persisted, retired, offered>>
  \* nothing else changes.

\* The retry ladder: with the marker set, send every current head of the document; once
\* all are acked and the heads have not moved, clear the marker and its failure witness.
Ladder(p) ==
  /\ marker[p] # {}
  \* the scope is dirty for p;
  /\ delivered' = [delivered EXCEPT ![p] = @ \cup Frontier]
  \* every current head is sent and acked;
  /\ marker' = [marker EXCEPT ![p] = {}]
  \* the marker clears;
  /\ persisted' = [persisted EXCEPT ![p] = {}]
  \* and with it the failure witness, which is the same durable key;
  /\ UNCHANGED <<nextHead, latest, queued, active, retired, offered>>
  \* nothing else changes.

Next ==
  \/ \E p \in Peers : Enqueue(p) \/ Ladder(p)
  \/ \E p \in Peers, h \in Heads :
       Start(p, h) \/ Fail(p, h) \/ Retry(p, h) \/ Ack(p, h)

Spec == Init /\ [][Next]_vars

INV_TypeOK == TypeOK

\* Safety: neither in-memory admission nor durable retry contains two heads
\* for one (scope, peer).
INV_OneLiveHead == \A p \in Peers : Cardinality(queued[p]) <= 1
INV_OnePersistedHead == \A p \in Peers : Cardinality(persisted[p]) <= 1

\* A persisted retry is always the newest head known for that peer.
INV_NoStaleRetry ==
  \A p \in Peers : persisted[p] \subseteq {latest[p]}

\* Liveness accounting: retirement never destroys the newest obligation.  It
\* remains queued, active, persisted, or has already been acknowledged.
INV_NewestRetained ==
  \A p \in Peers :
    latest[p] = 0 \/ latest[p] \in (queued[p] \cup active[p] \cup persisted[p])
                  \/ latest[p] \in Reach(delivered[p])

\* THE F1 INVARIANT: once a scope's marker is clear, every head ever offered to the peer
\* is delivered or an ancestor of a delivered head. A cleared marker with an undelivered
\* sibling is a head nothing will ever send again.
INV_ClearedScopeDelivered ==
  \A p \in Peers :
    marker[p] = {} => offered[p] \subseteq Reach(delivered[p])
====

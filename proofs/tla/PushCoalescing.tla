---- MODULE PushCoalescing ----
\* Latest-head retirement for outbound PushLog work (#1102), and the live
\* acknowledgement that completes a scope's durable marker (F1 sibling heads).
\*
\* THE STORY: a node commits heads of one document and offers each to every replicator
\* peer. The offer registers the scope's durable marker under a fresh generation, then
\* queues a live push job carrying that generation. The queue keeps only the newest head
\* per (scope, peer). A worker sends a job only while it is the newest live head and the
\* marker still holds its generation; the peer acks it or the send fails. The retry
\* ladder, while the marker is set, sends every current head of the document, records
\* them as the peer's acked heads and clears the marker. Heads form a DAG: a head links
\* its parents. Two heads merged from two different writers are siblings: neither links
\* the other.
\*
\* WHAT CAN GO WRONG:
\*   - head 1 is in flight, head 2 registers, then head 1 fails: the stale failure must
\*     not take the marker from head 2's job (#1102);
\*   - a live ack clears the marker, which stands for every head committed under it. If
\*     the acked head does not reach a sibling (retired from the queue, or fenced out of
\*     the live path by the newer generation), that sibling is never sent again (F1);
\*   - an ack that may not clear the marker still records its head as the peer's acked
\*     heads, with no generation to fence it. Landing after the ladder recorded the whole
\*     frontier, it overwrites that record with one head, and with the marker clear nothing
\*     rewrites it until the document moves again: the peer reads as unacknowledged.
\*
\* Every variable is code state (one document scope; per peer unless noted):
\*   nextHead    heads 1..nextHead-1 are committed: blocks in the local blockstore, and
\*               Frontier is what the headstore holds (DocumentHeadProvider) - local DB
\*   nextGen     the peerstore's generation allocator (/rep/retry/marker-generation) - local DB
\*   markerGen   the scope's retry marker (ReplicatorRetryDocIDKey): 0 when absent, else
\*               the generation it holds
\*   latest      PushBacklog Inner.latest for (peer, scope): the newest live head, 0 when
\*               absent
\*   queued      the peer's backlog queue: jobs <<head, marker_generation>> (PushJobSpec)
\*   active      jobs a push worker admitted and sent (run_push_job past admit_push_scope)
\*   delivered   heads the peer accepted, which it then owns with their DAG (the receiver's
\*               pending or merged heads: PushLogAdmission)
\*   ackedRecord the peer's acked-heads record (ReplicatorAckedHeadsKey): {} when absent
\*
\* Abstractions: one allocation per commit numbers that commit's registration for every
\* peer (a generation is compared only within its own marker key), and an allocation no
\* write uses is left out. Heads commit in (priority, CID) order, so the backlog never
\* retires an arriving head as stale. A commit and its registrations are one step. The
\* ladder's read, sends and fenced completion are one step: a registration in between
\* fails its fence and leaves the marker set.
\*
\* Code: crates/p2p/src/sync/coordinator/broadcast.rs (admit_replicator_push),
\* crates/p2p/src/sync/push_backlog.rs (BacklogOwner::enqueue, is_current, job_done),
\* crates/p2p/src/sync/coordinator/push_worker.rs (run_push_job, is_whole_frontier,
\* report_push_ack), crates/p2p-adapter/src/retry.rs (record_failure),
\* crates/storage/src/stores/peerstore.rs (observe_push_head_with_generation,
\* admit_push_scope, refresh_push_failure_with_generation, complete_retry_scope,
\* record_acknowledged_heads, acknowledged_heads), crates/db/src/merge/replication_ack.rs,
\* and the ladder in crates/db/src/merge/push_docs.rs (retry_doc,
\* complete_document_retry_if_current).
EXTENDS Naturals, FiniteSets

CONSTANTS
  Peers,        \* the replicator peers every head is offered to
  MaxHead,      \* heads 1..MaxHead are committed in this order
  Parent,       \* [1..MaxHead -> SUBSET (1..MaxHead)]: the heads each head links
  QueueMode,    \* "LatestOnly" | "AppendEvery"
  RetryMode,    \* "CurrentOnly" | "ReplayEvery"
  CompleteRule  \* "Frontier" | "Generation" | "Unfenced"

ASSUME Peers # {}
ASSUME MaxHead \in Nat /\ MaxHead >= 2
ASSUME QueueMode \in {"LatestOnly", "AppendEvery"}
ASSUME RetryMode \in {"CurrentOnly", "ReplayEvery"}
ASSUME CompleteRule \in {"Frontier", "Generation", "Unfenced"}
\* A head links only heads committed before it, so the parents form a DAG.
ASSUME \A h \in 1..MaxHead : Parent[h] \subseteq 1..(h - 1)

VARIABLES nextHead, nextGen, markerGen, latest, queued, active, delivered, ackedRecord

vars == <<nextHead, nextGen, markerGen, latest, queued, active, delivered, ackedRecord>>
Heads == 1..MaxHead

\* Each commit takes one generation, and each failure at most one more per peer: a job
\* fails at most once, and each peer gets one job per head.
MaxGen == MaxHead * (1 + Cardinality(Peers))
Gens == 1..MaxGen

\* A live push job: the head it sends and the marker generation it was queued with.
Jobs == Heads \X Gens

\* Every ancestor of h: its parents, their parents, and so on.
RECURSIVE AncOf(_)
AncOf(h) == Parent[h] \cup UNION {AncOf(p) : p \in Parent[h]}

\* The heads S holds, with every ancestor: what a peer holding S can reach.
Reach(S) == S \cup UNION {AncOf(h) : h \in S}

\* The heads committed so far.
Produced == 1..(nextHead - 1)

\* The document's current heads: committed heads no other committed head links.
Frontier == {h \in Produced : \A g \in Produced : h \notin AncOf(g)}

\* The heads a set of jobs sends.
HeadsOf(J) == {j[1] : j \in J}

\* The backlog's latest entry once job j is done: job_done removes it only if it is j's.
DoneLatest(p, j) == [latest EXCEPT ![p] = IF @ = j[1] THEN 0 ELSE @]

TypeOK ==
  /\ nextHead \in 1..(MaxHead + 1)
  /\ nextGen \in 1..(MaxGen + 1)
  /\ markerGen \in [Peers -> 0..MaxGen]
  /\ latest \in [Peers -> 0..MaxHead]
  /\ queued \in [Peers -> SUBSET Jobs]
  /\ active \in [Peers -> SUBSET Jobs]
  /\ delivered \in [Peers -> SUBSET Heads]
  /\ ackedRecord \in [Peers -> SUBSET Heads]

Init ==
  /\ nextHead = 1
  /\ nextGen = 1
  /\ markerGen = [p \in Peers |-> 0]
  /\ latest = [p \in Peers |-> 0]
  /\ queued = [p \in Peers |-> {}]
  /\ active = [p \in Peers |-> {}]
  /\ delivered = [p \in Peers |-> {}]
  /\ ackedRecord = [p \in Peers |-> {}]

\* A head commits and is offered to every peer. Its registration writes a fresh marker
\* generation, set or not, before its job is queued. LatestOnly retires the queued older
\* head of the (scope, peer); AppendEvery keeps every job.
Enqueue ==
  /\ nextHead <= MaxHead
  /\ LET job == <<nextHead, nextGen>>
     IN /\ markerGen' = [p \in Peers |-> nextGen]
        /\ latest' = [p \in Peers |-> nextHead]
        /\ queued' = [p \in Peers |->
             IF QueueMode = "LatestOnly" THEN {job} ELSE queued[p] \cup {job}]
  /\ nextHead' = nextHead + 1
  /\ nextGen' = nextGen + 1
  /\ UNCHANGED <<active, delivered, ackedRecord>>

\* A worker takes a queued job. It sends it only while the job is the newest live head
\* (is_current) and the marker still holds its generation (admit_push_scope); otherwise
\* the job ends unsent.
Start(p, j) ==
  /\ j \in queued[p]
  /\ queued' = [queued EXCEPT ![p] = @ \ {j}]
  /\ IF latest[p] = j[1] /\ markerGen[p] = j[2]
       THEN /\ active' = [active EXCEPT ![p] = @ \cup {j}]
            /\ UNCHANGED latest
       ELSE /\ latest' = DoneLatest(p, j)
            /\ UNCHANGED active
  /\ UNCHANGED <<nextHead, nextGen, markerGen, delivered, ackedRecord>>

\* The send fails. CurrentOnly: only the newest live head reports it, and the marker takes
\* a fresh generation, for the ladder, only while it still holds the job's. ReplayEvery
\* (#1102): every failure registers the marker afresh, current or not.
Fail(p, j) ==
  /\ j \in active[p]
  /\ active' = [active EXCEPT ![p] = @ \ {j}]
  /\ LET writes == RetryMode = "ReplayEvery" \/ (latest[p] = j[1] /\ markerGen[p] = j[2])
     IN /\ markerGen' = [markerGen EXCEPT ![p] = IF writes THEN nextGen ELSE @]
        /\ nextGen' = IF writes THEN nextGen + 1 ELSE nextGen
  /\ latest' = DoneLatest(p, j)
  /\ UNCHANGED <<nextHead, queued, delivered, ackedRecord>>

\* The peer accepts the head. Only the newest live head reports the ack. Frontier: only
\* when the head is the document's whole frontier, so every head committed before it is
\* one of its ancestors. Generation (before F1): always. Either way the ack carries the
\* job's generation and completes the marker, recording the head, only while the marker
\* still holds it. Unfenced (the first F1 patch): an ack that is not the whole frontier
\* is reported with no generation and recorded regardless.
Ack(p, j) ==
  /\ j \in active[p]
  /\ active' = [active EXCEPT ![p] = @ \ {j}]
  /\ delivered' = [delivered EXCEPT ![p] = @ \cup {j[1]}]
  /\ LET current == latest[p] = j[1]
         whole == Frontier = {j[1]}
         completes == current /\ (whole \/ CompleteRule = "Generation") /\ markerGen[p] = j[2]
         unfenced == current /\ ~whole /\ CompleteRule = "Unfenced"
     IN /\ markerGen' = [markerGen EXCEPT ![p] = IF completes THEN 0 ELSE @]
        /\ ackedRecord' = [ackedRecord EXCEPT ![p] =
             IF completes \/ unfenced THEN {j[1]} ELSE @]
  /\ latest' = DoneLatest(p, j)
  /\ UNCHANGED <<nextHead, nextGen, queued>>

\* The retry ladder, with the marker set: it sends every current head and, once all are
\* acked and the heads have not moved, clears the marker and records them as the peer's
\* acked heads in one transaction.
Ladder(p) ==
  /\ markerGen[p] # 0
  /\ delivered' = [delivered EXCEPT ![p] = @ \cup Frontier]
  /\ markerGen' = [markerGen EXCEPT ![p] = 0]
  /\ ackedRecord' = [ackedRecord EXCEPT ![p] = Frontier]
  /\ UNCHANGED <<nextHead, nextGen, latest, queued, active>>

Next ==
  \/ Enqueue
  \/ \E p \in Peers :
       \/ Ladder(p)
       \/ \E j \in queued[p] : Start(p, j)
       \/ \E j \in active[p] : Fail(p, j) \/ Ack(p, j)

Spec == Init /\ [][Next]_vars

INV_TypeOK == TypeOK

\* Safety: the live queue holds at most one job per (scope, peer).
INV_OneLiveHead == \A p \in Peers : Cardinality(queued[p]) <= 1

\* #1102: a stale send never takes the marker from the newest live head. While the
\* backlog's latest head is queued or in flight, the marker is clear or holds that job's
\* generation.
INV_NoStaleRetry ==
  \A p \in Peers : \A j \in queued[p] \cup active[p] :
    j[1] = latest[p] => markerGen[p] \in {0, j[2]}

\* Retirement never drops the newest committed head: it is queued or in flight, its
\* marker is set, or the peer holds it.
INV_NewestRetained ==
  \A p \in Peers :
    \/ nextHead = 1
    \/ nextHead - 1 \in HeadsOf(queued[p] \cup active[p])
    \/ markerGen[p] # 0
    \/ nextHead - 1 \in Reach(delivered[p])

\* THE F1 INVARIANT: once a scope's marker is clear, the peer holds every committed head
\* or a head that links it. A cleared marker with an undelivered sibling is a head nothing
\* will ever send again.
INV_ClearedScopeDelivered ==
  \A p \in Peers : markerGen[p] = 0 => Produced \subseteq Reach(delivered[p])

\* Once a scope's marker is clear, the peer's acked-heads record, when it has one, is the
\* document's current frontier: replication_acknowledged compares the two for equality,
\* and with the marker clear nothing rewrites the record until the document moves.
INV_AckRecordCurrent ==
  \A p \in Peers :
    markerGen[p] = 0 /\ ackedRecord[p] # {} => ackedRecord[p] = Frontier
====

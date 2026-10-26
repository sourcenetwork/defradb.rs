---- MODULE ReplicationAckScope ----
\* The story, in plain words.
\*
\* A node writes documents in one collection and pushes their new heads to a
\* replicator peer. The peer acknowledges each push it receives, and the node
\* records that acknowledgement under the scope the push had: one document's
\* heads, or the whole collection's heads. Later someone asks the node: "has
\* the peer acknowledged document d as it is now?" The node answers by reading
\* the acknowledgement recorded for d's scope and comparing it with the heads
\* it holds now.
\*
\* Which scope a push has depends on two things. A `@branchable` collection has
\* collection heads, and every write moves them. A replicator can be filtered:
\* it only gets the documents its filter matches. Every write pushes the
\* document's heads to each replicator that should get the document. A write in
\* a `@branchable` collection also pushes the collection heads, but only to
\* unfiltered replicators: the collection heads lead to every document, so a
\* filtered peer must never get them.
\*
\* So an unfiltered replicator of a `@branchable` collection acknowledges at
\* collection scope, and every other replicator acknowledges per document.
\*
\* The bug: the read picked collection scope for every `@branchable`
\* collection, filtered or not. A filtered replicator never gets the collection
\* heads, so nothing is ever recorded at collection scope for it, and its
\* documents read as never acknowledged even after the peer acknowledged them.
\*
\* The fix: one function picks the scope, and both the push and the read call it.
\*
\* What must never happen:
\*   * a document reads as acknowledged while the peer does not hold its
\*     current version;
\*   * a document that was never pushed reads as acknowledged;
\*   * a filtered peer receives a document its filter does not match.
\* And what must hold: a document whose current heads the peer acknowledged,
\* at the scope it was pushed at, reads as acknowledged.
\*
\* Code this models (defradb.rs):
\*   crates/p2p/src/ack_scope.rs    AckScope::new (the one scope rule)
\*   crates/p2p/src/replicator.rs   ReplicatorInfo::ack_scope
\*   crates/p2p/src/sync/coordinator/broadcast.rs
\*                                  SyncCoordinator::push_jobs (who gets a push)
\*   crates/db/src/merge/txn_broadcaster.rs
\*                                  broadcast (document push, then collection push)
\*   crates/storage/src/stores/peerstore.rs
\*                                  Peerstore::record_acknowledged_heads,
\*                                  Peerstore::acknowledged_heads
\*   crates/db/src/merge/replication_ack.rs
\*                                  replication_acknowledged (the read)
\*
\* Correspondence table (model -> Rust):
\*   PushScope       AckScope::new(filtered, branchable), reached through
\*                   ReplicatorInfo::ack_scope from both push_jobs and
\*                   replication_acknowledged
\*   Write           a committed write; txn_broadcaster.rs broadcast pushes the
\*                   document head, then the collection head (doc_id "") of a
\*                   `@branchable` collection; push_jobs picks the replicators
\*   GetsDoc         push_jobs: a filtered replicator needs `matches_filter`
\*   GetsCollection  push_jobs: a collection head (`push.doc_id.is_empty()`)
\*                   goes only to a replicator whose ack_scope is Collection
\*   Deliver         the peer acknowledges a push; the node records it with
\*                   record_acknowledged_heads, `is_collection` set when the
\*                   push's doc_id is empty
\*   ReadScope       replication_acknowledged: `scope == AckScope::Collection`
\*                   compares the collection heads, otherwise each document's
\*   MainRule        main before the fix: `collection.schema().is_branchable`
\*                   alone chose collection scope
\*   PushToFiltered  a broken push_jobs that sends collection heads to a
\*                   filtered replicator
\*   docHead, colHead  the heads this node holds (load_latest_composite_head_cids,
\*                   load_collection_head_cids), as version numbers
\*   ackDoc, ackCol  the ReplicatorAckedHeadsKey values per scope
\*   peerHas, covered  history variables, no Rust counterpart
\*
\* Boundaries: a filter's match is fixed per document here (a document whose
\* content moves it in or out of a filter is not modeled), and retry markers
\* (which make a scope read as unacknowledged while one is outstanding) are left
\* out; they only ever turn an answer into "not acknowledged".

EXTENDS Naturals

CONSTANTS
  Docs,           \* the documents in the collection
  Matches,        \* the documents a filtered replicator's filter matches
  MaxWrites,      \* how many writes each document may get, so the model is finite
  MainRule,       \* TRUE: the read uses main's rule (collection scope whenever branchable)
  PushToFiltered  \* TRUE: the push also sends collection heads to a filtered replicator

ASSUME Matches \subseteq Docs  \* a filter matches some of the documents

VARIABLES
  filtered,    \* whether the replicator has a filter on this collection
  branchable,  \* whether the collection is `@branchable`
  docHead,     \* each document's current version on this node; 0 means never written
  colHead,     \* the collection heads' version; moves on every write when branchable
  pending,     \* pushes sent and not yet acknowledged
  ackDoc,      \* per document, the version the peer last acknowledged at document scope
  ackCol,      \* the collection version the peer last acknowledged at collection scope
  peerHas,     \* history: the newest version of each document the peer holds
  covered      \* history: documents some push has carried

\* Every variable, for "nothing changed".
vars == <<filtered, branchable, docHead, colHead, pending, ackDoc, ackCol, peerHas, covered>>

\* The two kinds of push. A document push carries one document's version. A
\* collection push carries the collection version and, through the collection
\* heads, every document at the version it had when the push was sent.
DocPushes == [kind : {"doc"}, doc : Docs, ver : 1..MaxWrites]
ColPushes == [kind : {"col"}, ver : 1..(MaxWrites * 2), snap : [Docs -> 0..MaxWrites]]

\* The kinds of value each variable may hold.
TypeOK ==
  /\ filtered \in BOOLEAN                          \* filtered or not
  /\ branchable \in BOOLEAN                        \* branchable or not
  /\ docHead \in [Docs -> 0..MaxWrites]            \* a version per document
  /\ colHead \in Nat                               \* a version
  /\ pending \subseteq (DocPushes \cup ColPushes)  \* some pushes in flight
  /\ ackDoc \in [Docs -> 0..MaxWrites]             \* 0 means nothing recorded
  /\ ackCol \in Nat                                \* 0 means nothing recorded
  /\ peerHas \in [Docs -> 0..MaxWrites]            \* 0 means the peer has nothing
  /\ covered \subseteq Docs                        \* some of the documents

\* The one scope rule: an unfiltered replicator of a branchable collection is
\* pushed collection heads, so it acknowledges at collection scope; every
\* other replicator acknowledges per document. This is AckScope::new.
PushScope == IF branchable /\ ~filtered THEN "collection" ELSE "document"

\* Whether the replicator gets a document's own push: an unfiltered one always
\* does, a filtered one only for a document its filter matches.
GetsDoc(d) == ~filtered \/ d \in Matches

\* Whether the replicator gets the collection heads: only a branchable
\* collection has them, and only a collection-scope replicator gets them,
\* unless the broken push sends them to a filtered replicator too.
GetsCollection == branchable /\ (PushScope = "collection" \/ PushToFiltered)

\* The scope the read compares at: the push's rule, or main's rule, which looks
\* only at whether the collection is branchable.
ReadScope ==
  IF MainRule
    THEN IF branchable THEN "collection" ELSE "document"  \* main before the fix
    ELSE PushScope                                         \* the fix

\* The read for document d: a document never written is never acknowledged;
\* otherwise the recorded acknowledgement at the read's scope must equal the
\* version this node holds now.
Acknowledged(d) ==
  /\ docHead[d] > 0                       \* the document exists
  /\ IF ReadScope = "collection"
       THEN ackCol = colHead              \* the collection heads were acknowledged
       ELSE ackDoc[d] = docHead[d]        \* this document's heads were acknowledged

\* The peer acknowledged d's current heads at the scope d is pushed at. For a
\* collection-scope replicator, a document is current only together with the
\* collection heads, because those are what it is pushed at.
CurrentAcked(d) ==
  /\ docHead[d] > 0                           \* the document exists
  /\ IF PushScope = "collection"
       THEN ackCol = colHead                  \* the newest collection push was acknowledged
       ELSE ackDoc[d] = docHead[d]            \* the newest document push was acknowledged

\* At the start the replicator and the collection take any of the four
\* shapes, nothing is written, nothing is pushed and nothing acknowledged.
Init ==
  /\ filtered \in BOOLEAN                 \* filtered or not
  /\ branchable \in BOOLEAN               \* branchable or not
  /\ docHead = [d \in Docs |-> 0]         \* no document written
  /\ colHead = 0                          \* no collection heads yet
  /\ pending = {}                         \* no push in flight
  /\ ackDoc = [d \in Docs |-> 0]          \* nothing recorded per document
  /\ ackCol = 0                           \* nothing recorded for the collection
  /\ peerHas = [d \in Docs |-> 0]         \* the peer has nothing
  /\ covered = {}                         \* no push has carried anything

\* A write to document d commits. Its heads move, and the collection heads too
\* when the collection is branchable. Then the pushes go out: the document's
\* own, if the replicator gets it, and the collection's, if it gets those.
Write(d) ==
  /\ docHead[d] < MaxWrites                                     \* within the allowance
  /\ LET nd == [docHead EXCEPT ![d] = docHead[d] + 1]           \* the new document versions
         nc == IF branchable THEN colHead + 1 ELSE colHead      \* the new collection version
         dp == IF GetsDoc(d)
                 THEN {[kind |-> "doc", doc |-> d, ver |-> nd[d]]}  \* the document push
                 ELSE {}                                            \* filtered out
         cp == IF GetsCollection
                 THEN {[kind |-> "col", ver |-> nc, snap |-> nd]}   \* the collection push
                 ELSE {}                                            \* not sent
     IN /\ docHead' = nd                                        \* the document moved
        /\ colHead' = nc                                        \* the collection moved
        /\ pending' = pending \cup dp \cup cp                   \* the pushes are in flight
        /\ covered' = covered
                      \cup (IF GetsDoc(d) THEN {d} ELSE {})     \* the document push carries d
                      \cup (IF GetsCollection
                              THEN {e \in Docs : nd[e] > 0}     \* the collection push carries all written
                              ELSE {})
  /\ UNCHANGED <<filtered, branchable, ackDoc, ackCol, peerHas>>

\* The peer receives one push, in any order, and acknowledges it. The node
\* records the acknowledgement at the push's scope. The peer now holds what the
\* push carried.
Deliver(p) ==
  /\ p \in pending                                              \* a push in flight
  /\ pending' = pending \ {p}                                   \* it is done
  /\ IF p.kind = "doc"
       THEN /\ ackDoc' = [ackDoc EXCEPT ![p.doc] = p.ver]       \* recorded per document
            /\ peerHas' = [peerHas EXCEPT ![p.doc] =
                             IF p.ver > @ THEN p.ver ELSE @]    \* the peer keeps the newest
            /\ UNCHANGED ackCol
       ELSE /\ ackCol' = p.ver                                  \* recorded for the collection
            /\ peerHas' = [e \in Docs |->
                             IF p.snap[e] > peerHas[e]
                               THEN p.snap[e] ELSE peerHas[e]]  \* every document it reaches
            /\ UNCHANGED ackDoc
  /\ UNCHANGED <<filtered, branchable, docHead, colHead, covered>>

\* Every write is done and every push delivered.
Done == pending = {} /\ \A d \in Docs : docHead[d] = MaxWrites

\* One step: a write, or a delivery. When all is done nothing more happens,
\* which is not a deadlock.
Next ==
  \/ \E d \in Docs : Write(d)      \* some document is written
  \/ \E p \in pending : Deliver(p) \* or some push is acknowledged
  \/ Done /\ UNCHANGED vars        \* or everything has finished

\* The whole behaviour: start, then any sequence of steps.
Spec == Init /\ [][Next]_vars

\* A document whose current heads the peer acknowledged reads as acknowledged.
INV_AckedReadsAcked == \A d \in Docs : CurrentAcked(d) => Acknowledged(d)

\* A document that reads as acknowledged is held by the peer at its current version.
INV_AckedIsHeld == \A d \in Docs : Acknowledged(d) => peerHas[d] = docHead[d]

\* A document no push ever carried never reads as acknowledged.
INV_NeverPushedNeverAcked == \A d \in Docs : d \notin covered => ~Acknowledged(d)

\* A filtered peer never holds a document its filter does not match.
INV_FilterRespected == filtered => \A d \in Docs \ Matches : peerHas[d] = 0
====

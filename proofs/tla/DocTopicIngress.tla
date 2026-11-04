---- MODULE DocTopicIngress ----
\* The story, in plain words.
\*
\* A node follows one document: it subscribed to the document's gossip topic
\* (P2PAdapter::add_documents in crates/p2p-adapter/src/libp2p.rs and
\* iroh.rs, or the restore at startup, restore_libp2p_documents and
\* restore_iroh_p2p_state), putting the document in the shared set first. The document's publisher sends an update now and then. The
\* node's transport hands each update on the topic to the sync coordinator
\* through a queue, and the coordinator takes them one at a time
\* (SyncCoordinator::handle_gossip_message in
\* crates/p2p/src/sync/coordinator/event_handler/gossip.rs).
\*
\* Then the node unsubscribes (P2PAdapter::remove_documents): it takes the
\* document out of its set of followed documents, asks its transport to leave
\* the topic, and returns. The transport leaves the topic some time later, so
\* updates may still reach the queue, and updates already queued are taken
\* after the call returned. The node may also follow the document's whole
\* collection, whose subscription covers the document as well.
\*
\* What must never happen:
\*   1. once the unsubscribe returned, an update for the document is applied
\*      while the node does not follow its collection - INV_NoApplyAfterUnsubscribe;
\*   2. an update taken while the node follows the document or its collection
\*      is dropped - INV_FollowedApplies.
\*
\* The Rust code each guard models:
\*   CheckLive   handle_gossip_message accepts a message on a document's topic
\*               only while the document or its collection is followed;
\*   SharedSet   the set the handler looks at is the one the adapter's
\*               unsubscribe clears (the coordinator's set, which the adapter
\*               shares), not a copy of its own;
\*   Collection  a followed collection also admits the document's topic.

EXTENDS Naturals, Sequences

CONSTANTS
  MaxMsgs,     \* how many updates the publisher sends in one run
  CheckLive,   \* TRUE: the handler checks the document is followed
  SharedSet,   \* TRUE: the handler checks the set the unsubscribe clears
  Collection   \* TRUE: a followed collection admits the document's topic

VARIABLES
  followed,    \* the document is in the shared set of followed documents
  stale,       \* the handler's own copy of that set (read only when SharedSet is broken)
  coll,        \* the node follows the document's collection (fixed for the run)
  joined,      \* the transport is on the document's topic
  call,        \* the unsubscribe: "none", "cleared" (set changed, transport asked) or "returned"
  queue,       \* updates handed to the coordinator, oldest first: each says whether it was sent after the call returned
  sent,        \* updates the publisher sent so far
  lateApplied, \* (record) an update sent after the call returned was applied without the collection
  liveDropped  \* (record) an update taken while followed was dropped

vars == <<followed, stale, coll, joined, call, queue, sent, lateApplied, liveDropped>>  \* every variable

Init ==                            \* at the start:
  /\ followed = TRUE               \* the node follows the document
  /\ stale = TRUE                  \* and its copy says so too
  /\ coll \in BOOLEAN              \* it may or may not follow the collection
  /\ joined = TRUE                 \* the transport is on the topic
  /\ call = "none"                 \* no unsubscribe yet
  /\ queue = <<>>                  \* nothing queued
  /\ sent = 0                      \* nothing sent
  /\ lateApplied = FALSE           \* nothing went wrong
  /\ liveDropped = FALSE           \* nothing went wrong

\* The publisher sends an update; while the transport is on the topic, it is queued for the coordinator.
Send ==
  /\ sent < MaxMsgs                                   \* within the run's bound
  /\ sent' = sent + 1                                 \* one more sent
  /\ queue' = IF joined THEN Append(queue, call = "returned") ELSE queue  \* queued, marked late after the return
  /\ UNCHANGED <<followed, stale, coll, joined, call, lateApplied, liveDropped>>  \* the rest stays

\* The unsubscribe begins: the document leaves the set, and the transport is asked to leave the topic.
Clear ==
  /\ call = "none"                                    \* once
  /\ call' = "cleared"                                \* the first step done
  /\ followed' = FALSE                                \* out of the shared set
  /\ UNCHANGED <<stale, coll, joined, queue, sent, lateApplied, liveDropped>>  \* the handler's own copy is not touched

\* The unsubscribe returns to its caller.
Return ==
  /\ call = "cleared"                                 \* after its first step
  /\ call' = "returned"                               \* returned
  /\ UNCHANGED <<followed, stale, coll, joined, queue, sent, lateApplied, liveDropped>>  \* the rest stays

\* The transport leaves the topic, at any moment after it was asked.
Leave ==
  /\ call # "none" /\ joined                          \* asked, still on it
  /\ joined' = FALSE                                  \* off the topic
  /\ UNCHANGED <<followed, stale, coll, call, queue, sent, lateApplied, liveDropped>>  \* the rest stays

\* The coordinator takes the oldest queued update and accepts or drops it.
Take ==
  /\ queue # <<>>                                     \* something queued
  /\ LET late == Head(queue)                          \* sent after the call returned
         doc == IF SharedSet THEN followed ELSE stale \* MUTATION shared_set: the handler reads its own copy
         col == Collection /\ coll                    \* MUTATION collection: the collection does not admit it
         accept == ~CheckLive \/ doc \/ col           \* MUTATION check_live: everything on the topic is accepted
         live == followed \/ coll                     \* the truth: the node follows the document or its collection
     IN /\ lateApplied' = (lateApplied \/ (accept /\ late /\ ~coll))  \* recorded if a late update was applied
        /\ liveDropped' = (liveDropped \/ (~accept /\ live))         \* recorded if a followed update was dropped
  /\ queue' = Tail(queue)                             \* taken
  /\ UNCHANGED <<followed, stale, coll, joined, call, sent>>  \* the rest stays

Next == Send \/ Clear \/ Return \/ Leave \/ Take       \* any of them, in any order
Spec == Init /\ [][Next]_vars                         \* every behaviour

TypeOK ==
  /\ followed \in BOOLEAN
  /\ stale \in BOOLEAN
  /\ coll \in BOOLEAN
  /\ joined \in BOOLEAN
  /\ call \in {"none", "cleared", "returned"}
  /\ queue \in Seq(BOOLEAN)
  /\ sent \in 0..MaxMsgs
  /\ lateApplied \in BOOLEAN
  /\ liveDropped \in BOOLEAN

\* Once the unsubscribe returned, no update for the document is applied unless its collection is followed.
INV_NoApplyAfterUnsubscribe == ~lateApplied
\* An update taken while the node follows the document or its collection is applied.
INV_FollowedApplies == ~liveDropped
====

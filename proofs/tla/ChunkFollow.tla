---- MODULE ChunkFollow ----
\* The story: a source node writes articles. Each article has a `@chunk` text,
\* so writing it also writes its chunk documents in a second collection. A
\* peer gets articles two ways. A replicator with a filter pushes it every
\* write of an article the filter matches. And the peer can ask for one
\* article by its ID (a document sync).
\*
\* The filter names the article's own fields. A chunk has none of them, so
\* the push of a chunk must be decided by its article, and a document sync of
\* an article must bring the article's chunks with it.
\*
\* The source can also edit an article so its text no longer uses one of its
\* chunks. The edit deletes that chunk in the same transaction, and the delete
\* is pushed the way the chunk's create was, decided by its article. A document
\* sync answers only with the chunks the article still uses, so it never brings
\* that delete. The peer's merge of the edited article deletes the chunk its
\* text no longer uses, and drops a create of it that arrives later.
\*
\* What must never happen:
\*   - the peer ends up with a live article but not all of its chunks
\*     (INV_ChunksFollowParent);
\*   - the peer holds a chunk of an article it was never sent nor asked for
\*     (INV_NoForeignChunks);
\*   - a matching article's delete reaches the peer and its chunks stay live
\*     there (INV_DeletesFollowParent);
\*   - an edit reaches the peer and the chunk it stopped using stays live there
\*     (INV_DroppedChunksGo).
\*
\* Code it models:
\*   crates/p2p/src/sync/coordinator/broadcast.rs  SyncCoordinator::push_jobs
\*     (a chunk's push is decided by its parent's filter)
\*   crates/db/src/merge/push_docs/follow.rs  replica_takes (replay and retry decide a
\*     chunk by its parent too)
\*   crates/db/src/chunk/sync/write.rs  ChunkWrites::publish (each chunk head,
\*     create or delete, carries its parent)
\*   crates/db/src/chunk/sync/mod.rs  ChunkWrites::sync_chunks (an edit deletes
\*     the chunks its text no longer uses)
\*   crates/db/src/chunk/rebuild.rs  reconcile (a merge deletes them too)
\*   crates/db/src/chunk/peer_create.rs  create_is_stale (a late create is dropped)
\*   crates/p2p/src/sync/coordinator/event_handler/doc_sync.rs
\*     with_followers (a synced article answers with its chunks)
\*   crates/db/src/merge/head_provider.rs  DbHeadProvider::following_doc_ids
EXTENDS Naturals, FiniteSets

CONSTANTS
  Articles,      \* the articles the source may write, for example {pub, priv}
  Matching,      \* the articles the replicator's filter matches
  ChunkRule,     \* how a chunk's push is decided: "parent" (the fix), "own" or "all"
  SyncFollows,   \* TRUE: a document sync of an article also brings its chunks (the fix)
  DeletesFollow, \* TRUE: an edit's chunk delete is pushed like the chunk's create (the fix)
  Reconcile      \* TRUE: the peer's merge of an edit deletes the chunk it stopped using (the fix)

\* The filter matches only some of the articles.
ASSUME Matching \subseteq Articles
\* The three ways a chunk's push can be decided.
ASSUME ChunkRule \in {"parent", "own", "all"}
\* The switches are yes-or-no.
ASSUME SyncFollows \in BOOLEAN /\ DeletesFollow \in BOOLEAN /\ Reconcile \in BOOLEAN

\* Each article has two chunks, named by the article and a number.
Chunks(a) == {<<a, 1>>, <<a, 2>>}
\* Every chunk of every article.
AllChunks == UNION {Chunks(a) : a \in Articles}
\* The article a chunk belongs to.
Owner(c) == c[1]
\* The chunk an edit stops using.
Dropped(a) == <<a, 2>>

\* A document is missing, live, edited (an article only) or deleted, in that order.
States == {"none", "live", "edited", "deleted"}
\* The rank of a state, so a later state wins over an earlier one.
Rank(s) == CASE s = "none" -> 0 [] s = "live" -> 1 [] s = "edited" -> 2 [] s = "deleted" -> 3
\* The later of two states: a peer never goes back to an older one.
Later(s, t) == IF Rank(t) > Rank(s) THEN t ELSE s

\* The chunks an article in state s uses: both while live, one once edited.
Uses(a, s) == CASE s = "live" -> Chunks(a)
                [] s = "edited" -> Chunks(a) \ {Dropped(a)}
                [] OTHER -> {}

VARIABLES
  src,       \* each article's state on the source
  peerDoc,   \* each article's state on the peer
  peerChunk, \* each chunk's state on the peer
  wire,      \* pushes sent and not yet delivered: [doc |-> d, state |-> s]
  asked      \* the articles the peer asked for by a document sync

\* All the variables, for stuttering steps.
vars == <<src, peerDoc, peerChunk, wire, asked>>

\* The type of every variable.
TypeOK ==
  /\ src \in [Articles -> States]                     \* source states
  /\ peerDoc \in [Articles -> States]                 \* the peer's articles
  /\ peerChunk \in [AllChunks -> States]              \* the peer's chunks
  /\ wire \subseteq [doc : Articles \cup AllChunks, state : States]  \* pushes
  /\ asked \subseteq Articles                         \* the synced articles

\* Nothing is written, nothing is on the peer, nothing is in flight.
Init ==
  /\ src = [a \in Articles |-> "none"]       \* no article yet
  /\ peerDoc = [a \in Articles |-> "none"]   \* the peer has none
  /\ peerChunk = [c \in AllChunks |-> "none"]  \* nor any chunk
  /\ wire = {}                               \* no push in flight
  /\ asked = {}                              \* no sync asked for

\* Whether the filtered replicator is pushed this chunk.
ChunkSent(c) ==
  CASE ChunkRule = "parent" -> Owner(c) \in Matching  \* the fix: its article decides
    [] ChunkRule = "own" -> FALSE                      \* bug: a chunk never matches the filter
    [] ChunkRule = "all" -> TRUE                       \* bug: every chunk goes, matching or not

\* The pushes one write of article a sends: the article if the filter
\* matches it, and each of the chunks `cs` the chunk rule sends, all in state s.
Pushes(a, cs, s) ==
  (IF a \in Matching THEN {[doc |-> a, state |-> s]} ELSE {})  \* the article
  \cup {[doc |-> c, state |-> s] : c \in {k \in cs : ChunkSent(k)}}  \* its chunks

\* The peer's chunks after its article moves to state s: with Reconcile, a
\* live chunk the article no longer uses is deleted.
Reconciled(a, s, chunks) ==
  [c \in AllChunks |->
     IF Reconcile /\ s = "edited" /\ c = Dropped(a) /\ chunks[c] = "live"
     THEN "deleted" ELSE chunks[c]]

\* The source creates article a with its chunks in one transaction.
Create(a) ==
  /\ src[a] = "none"                         \* a is new
  /\ src' = [src EXCEPT ![a] = "live"]       \* a is live on the source
  /\ wire' = wire \cup Pushes(a, Chunks(a), "live")  \* its pushes are sent
  /\ UNCHANGED <<peerDoc, peerChunk, asked>>  \* the peer has not seen them yet

\* The source edits article a so it stops using one chunk, and deletes that
\* chunk in the same transaction.
Edit(a) ==
  /\ src[a] = "live"                          \* a is live and not edited yet
  /\ src' = [src EXCEPT ![a] = "edited"]      \* a is edited on the source
  /\ wire' = wire \cup Pushes(a, {}, "edited")  \* the article's push is sent,
           \cup (IF DeletesFollow                \* and with the fix the chunk's delete,
                 THEN {m \in Pushes(a, {Dropped(a)}, "deleted") : m.doc = Dropped(a)}  \* sent as its create was
                 ELSE {})                         \* or without it, no delete at all
  /\ UNCHANGED <<peerDoc, peerChunk, asked>>  \* the peer has not seen them yet

\* The source deletes article a, and its chunks in the same transaction.
Delete(a) ==
  /\ src[a] \in {"live", "edited"}                     \* a is live
  /\ src' = [src EXCEPT ![a] = "deleted"]               \* a is deleted on the source
  /\ wire' = wire \cup Pushes(a, Uses(a, src[a]), "deleted")  \* the deletes are sent
  /\ UNCHANGED <<peerDoc, peerChunk, asked>>            \* the peer has not seen them yet

\* One push reaches the peer, in any order.
Deliver(m) ==
  /\ m \in wire                               \* it was sent
  /\ wire' = wire \ {m}                       \* it is no longer in flight
  /\ IF m.doc \in Articles                    \* an article's push
     THEN LET s == Later(peerDoc[m.doc], m.state)  \* the article's new state
          IN /\ peerDoc' = [peerDoc EXCEPT ![m.doc] = s]  \* updates the article
             /\ peerChunk' = Reconciled(m.doc, s, peerChunk)  \* and its merge cleans up
     ELSE LET a == Owner(m.doc)               \* a chunk's push
              stale == Reconcile /\ m.state = "live" /\ m.doc \notin Uses(a, peerDoc[a])
                       /\ peerDoc[a] \in {"edited"}  \* a create the edited article does not use
          IN /\ peerChunk' = IF stale THEN peerChunk  \* is dropped
                             ELSE [peerChunk EXCEPT ![m.doc] = Later(@, m.state)]  \* or updates the chunk
             /\ UNCHANGED peerDoc
  /\ UNCHANGED <<src, asked>>                 \* the source does not move

\* The peer syncs article a by its ID. The source answers with the article's
\* heads and, with the fix, the heads of the chunks its index still names,
\* which are the chunks a live article uses.
Sync(a) ==
  /\ src[a] # "none"                          \* the source has the article
  /\ asked' = asked \cup {a}                  \* the peer asked for it
  /\ LET s == Later(peerDoc[a], src[a])        \* the article's new state on the peer
         brought == IF SyncFollows THEN Uses(a, src[a]) ELSE {}  \* the chunks that arrive
         got == [c \in AllChunks |->
                   IF c \in brought THEN Later(peerChunk[c], "live") ELSE peerChunk[c]]
     IN /\ peerDoc' = [peerDoc EXCEPT ![a] = s]  \* the article arrives
        /\ peerChunk' = Reconciled(a, s, got)    \* with its chunks, cleaned up
  /\ UNCHANGED <<src, wire>>                  \* nothing new is pushed

\* Any of the steps above.
Next ==
  \/ \E a \in Articles : Create(a) \/ Edit(a) \/ Delete(a) \/ Sync(a)  \* the source writes or the peer syncs
  \/ \E m \in wire : Deliver(m)                                      \* or a push arrives

\* The whole behaviour.
Spec == Init /\ [][Next]_vars

\* Every push sent has arrived.
Quiet == wire = {}

\* Once quiet, a live article on the peer has every chunk it uses live with it.
INV_ChunksFollowParent ==
  Quiet => \A a \in Articles :
    \A c \in Uses(a, peerDoc[a]) : peerChunk[c] = "live"

\* A chunk reaches the peer only if its article matches the filter or the
\* peer asked for that article.
INV_NoForeignChunks ==
  \A c \in AllChunks :
    peerChunk[c] # "none" => Owner(c) \in Matching \cup asked

\* Once quiet, a matching article deleted on the peer leaves none of its
\* chunks live there.
INV_DeletesFollowParent ==
  Quiet => \A a \in Matching :
    peerDoc[a] = "deleted" => \A c \in Chunks(a) : peerChunk[c] # "live"

\* Once quiet, an edited article on the peer leaves the chunk it stopped
\* using not live there.
INV_DroppedChunksGo ==
  Quiet => \A a \in Articles :
    peerDoc[a] = "edited" => peerChunk[Dropped(a)] # "live"
====

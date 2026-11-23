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
\* What must never happen:
\*   - the peer ends up with a live article but not all of its chunks
\*     (INV_ChunksFollowParent);
\*   - the peer holds a chunk of an article it was never sent nor asked for
\*     (INV_NoForeignChunks);
\*   - a matching article's delete reaches the peer and its chunks stay live
\*     there (INV_DeletesFollowParent).
\*
\* Code it models:
\*   crates/database/p2p/src/sync/coordinator/broadcast.rs  SyncCoordinator::push_jobs
\*     (a chunk's push is decided by its parent's filter)
\*   crates/database/db/src/merge/push_docs/follow.rs  replica_takes (replay and retry decide a
\*     chunk by its parent too)
\*   crates/database/db/src/chunk/sync/write.rs  ChunkWrites::publish (each chunk head
\*     carries its parent)
\*   crates/database/p2p/src/sync/coordinator/event_handler/doc_sync.rs
\*     with_followers (a synced article answers with its chunks)
\*   crates/database/db/src/merge/head_provider.rs  DbHeadProvider::following_doc_ids
EXTENDS Naturals, FiniteSets

CONSTANTS
  Articles,     \* the articles the source may write, for example {pub, priv}
  Matching,     \* the articles the replicator's filter matches
  ChunkRule,    \* how a chunk's push is decided: "parent" (the fix), "own" or "all"
  SyncFollows   \* TRUE: a document sync of an article also brings its chunks (the fix)

\* The filter matches only some of the articles.
ASSUME Matching \subseteq Articles
\* The three ways a chunk's push can be decided.
ASSUME ChunkRule \in {"parent", "own", "all"}
\* The sync switch is yes-or-no.
ASSUME SyncFollows \in BOOLEAN

\* Each article has two chunks, named by the article and a number.
Chunks(a) == {<<a, 1>>, <<a, 2>>}
\* Every chunk of every article.
AllChunks == UNION {Chunks(a) : a \in Articles}
\* The article a chunk belongs to.
Owner(c) == c[1]

\* A document is missing, live or deleted, in that order.
States == {"none", "live", "deleted"}
\* The rank of a state, so a later state wins over an earlier one.
Rank(s) == CASE s = "none" -> 0 [] s = "live" -> 1 [] s = "deleted" -> 2
\* The later of two states: a peer never goes back to an older one.
Later(s, t) == IF Rank(t) > Rank(s) THEN t ELSE s

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
\* matches it, and each chunk the chunk rule sends, all in state s.
Pushes(a, s) ==
  (IF a \in Matching THEN {[doc |-> a, state |-> s]} ELSE {})  \* the article
  \cup {[doc |-> c, state |-> s] : c \in {k \in Chunks(a) : ChunkSent(k)}}  \* its chunks

\* The source creates article a with its chunks in one transaction.
Create(a) ==
  /\ src[a] = "none"                         \* a is new
  /\ src' = [src EXCEPT ![a] = "live"]       \* a is live on the source
  /\ wire' = wire \cup Pushes(a, "live")     \* its pushes are sent
  /\ UNCHANGED <<peerDoc, peerChunk, asked>>  \* the peer has not seen them yet

\* The source deletes article a, and its chunks in the same transaction.
Delete(a) ==
  /\ src[a] = "live"                          \* a is live
  /\ src' = [src EXCEPT ![a] = "deleted"]     \* a is deleted on the source
  /\ wire' = wire \cup Pushes(a, "deleted")   \* the deletes are sent
  /\ UNCHANGED <<peerDoc, peerChunk, asked>>  \* the peer has not seen them yet

\* One push reaches the peer, in any order.
Deliver(m) ==
  /\ m \in wire                               \* it was sent
  /\ wire' = wire \ {m}                       \* it is no longer in flight
  /\ IF m.doc \in Articles                    \* an article's push
     THEN /\ peerDoc' = [peerDoc EXCEPT ![m.doc] = Later(@, m.state)]  \* updates the article
          /\ UNCHANGED peerChunk              \* and no chunk
     ELSE /\ peerChunk' = [peerChunk EXCEPT ![m.doc] = Later(@, m.state)]  \* a chunk's push
          /\ UNCHANGED peerDoc                \* updates only that chunk
  /\ UNCHANGED <<src, asked>>                 \* the source does not move

\* The peer syncs article a by its ID. The source answers with the article's
\* heads and, with the fix, the heads of the chunks its index still names,
\* which are the chunks of a live article.
Sync(a) ==
  /\ src[a] # "none"                          \* the source has the article
  /\ asked' = asked \cup {a}                  \* the peer asked for it
  /\ peerDoc' = [peerDoc EXCEPT ![a] = Later(@, src[a])]  \* the article arrives
  /\ IF SyncFollows /\ src[a] = "live"        \* with the fix, for a live article,
     THEN peerChunk' = [c \in AllChunks |->   \* its chunks arrive too
            IF c \in Chunks(a) THEN Later(peerChunk[c], "live") ELSE peerChunk[c]]
     ELSE UNCHANGED peerChunk                 \* with the bug, only the article does
  /\ UNCHANGED <<src, wire>>                  \* nothing new is pushed

\* Any of the steps above.
Next ==
  \/ \E a \in Articles : Create(a) \/ Delete(a) \/ Sync(a)  \* the source writes or the peer syncs
  \/ \E m \in wire : Deliver(m)                            \* or a push arrives

\* The whole behaviour.
Spec == Init /\ [][Next]_vars

\* Every push sent has arrived.
Quiet == wire = {}

\* Once quiet, a live article on the peer has every chunk live with it.
INV_ChunksFollowParent ==
  Quiet => \A a \in Articles :
    peerDoc[a] = "live" => \A c \in Chunks(a) : peerChunk[c] = "live"

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
====

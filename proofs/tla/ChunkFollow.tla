---- MODULE ChunkFollow ----
\* The story: a source node writes articles. Each article has a `@chunk` text,
\* so writing it also writes its chunk documents in a second collection. The
\* source can edit an article's text later. The edit writes the chunks of the
\* new text, and the old text's chunks stay stored: readers hide them, and
\* only a delete of the article removes them. A peer gets articles two ways.
\* A replicator with a filter pushes it every write of an article the filter
\* matches. And the peer can ask for one article by its ID (a document sync).
\*
\* The filter names the article's own fields. A chunk has none of them, so
\* the push of a chunk must be decided by its article, and a document sync of
\* an article must bring the article's chunks with it. Only the chunks of the
\* article's current text come with a sync: the old ones grow with every edit,
\* so sending them too would make one reply grow with the article's history.
\*
\* What must never happen:
\*   - the peer ends up with a live article but not all of the chunks of the
\*     text it holds (INV_ChunksFollowParent);
\*   - the peer holds a chunk of an article it was never sent nor asked for
\*     (INV_NoForeignChunks);
\*   - a matching article's delete reaches the peer and its chunks stay live
\*     there (INV_DeletesFollowParent);
\*   - a document sync answers with a chunk the article's current text does
\*     not use (INV_SyncSendsCurrentChunks).
\*
\* Code it models:
\*   crates/database/p2p/src/sync/coordinator/broadcast.rs  SyncCoordinator::push_jobs
\*     (a chunk's push is decided by its parent's filter)
\*   crates/database/db/src/merge/push_docs/follow.rs  ReplicaFilter::decide (replay and retry
\*     decide a chunk by its parent too)
\*   crates/database/db/src/chunk/sync/write.rs  ChunkWrites::publish (each chunk head
\*     carries its parent)
\*   crates/database/p2p/src/sync/doc_sync_ids.rs  with_followers (the one place a synced
\*     article adds its chunks, used by both doc sync handlers below)
\*   crates/database/p2p/src/sync/coordinator/event_handler/doc_sync.rs
\*     SyncCoordinator::handle_doc_sync_request (the stream doc sync)
\*   crates/database/p2p/src/sync/coordinator/pubsub_services.rs  DocSyncHandler::handle
\*     (the pubsub doc sync)
\*   crates/database/db/src/merge/head_provider.rs  DbHeadProvider::following_doc_ids
\*   crates/database/db/src/chunk/follow.rs  current_chunk_ids (cut the current text, look up
\*     each chunk by parent and hash; a deleted parent has none)
EXTENDS Naturals, FiniteSets

CONSTANTS
  Articles,     \* the articles the source may write, for example {pub, priv}
  Matching,     \* the articles the replicator's filter matches
  ChunkRule,    \* how a chunk's push is decided: "parent" (the fix), "own" or "all"
  SyncRule      \* what a document sync sends with an article: "current" (the fix), "history" or "none"

\* The filter matches only some of the articles.
ASSUME Matching \subseteq Articles
\* The three ways a chunk's push can be decided.
ASSUME ChunkRule \in {"parent", "own", "all"}
\* The three ways a document sync can answer.
ASSUME SyncRule \in {"current", "history", "none"}

\* An article's text has two versions: the one it is created with, and the
\* one a single edit writes.
Versions == {1, 2}
\* The chunk numbers each version is cut into. They share chunk 2, as an edit
\* of one paragraph keeps the others. No text yet has no chunks.
ChunkNums(v) == CASE v = 1 -> {1, 2}
                  [] v = 2 -> {2, 3}
                  [] OTHER -> {}
\* The chunks of article a's text at version v, named by article and number.
Chunks(a, v) == {<<a, n>> : n \in ChunkNums(v)}
\* Every chunk of every article.
AllChunks == Articles \X {1, 2, 3}
\* The article a chunk belongs to.
Owner(c) == c[1]

\* A document is missing, live or deleted, in that order.
States == {"none", "live", "deleted"}
\* The rank of a state, so a later state wins over an earlier one.
Rank(s) == CASE s = "none" -> 0 [] s = "live" -> 1 [] s = "deleted" -> 2
\* The later of two states: a peer never goes back to an older one.
Later(s, t) == IF Rank(t) > Rank(s) THEN t ELSE s
\* The larger of two versions: the edit's text wins over the first text.
Max(x, y) == IF x > y THEN x ELSE y

VARIABLES
  src,       \* each article's state on the source
  srcText,   \* the version of each article's text on the source, 0 before it exists
  written,   \* written[a]: every chunk of a the source has stored, old ones too
  peerDoc,   \* each article's state on the peer
  peerText,  \* the version of each article's text on the peer, 0 before it arrives
  peerChunk, \* each chunk's state on the peer
  wire,      \* pushes sent and not yet delivered: [doc |-> d, state |-> s, text |-> v]
  asked,     \* the articles the peer asked for by a document sync
  replies    \* every sync answer: the article, its text then, and the chunks sent

\* All the variables, for stuttering steps.
vars == <<src, srcText, written, peerDoc, peerText, peerChunk, wire, asked, replies>>

\* The type of every variable.
TypeOK ==
  /\ src \in [Articles -> States]                     \* source states
  /\ srcText \in [Articles -> {0} \cup Versions]      \* source texts
  /\ written \in [Articles -> SUBSET AllChunks]       \* chunks stored on the source
  /\ peerDoc \in [Articles -> States]                 \* the peer's articles
  /\ peerText \in [Articles -> {0} \cup Versions]     \* the texts the peer holds
  /\ peerChunk \in [AllChunks -> States]              \* the peer's chunks
  /\ wire \subseteq [doc : Articles \cup AllChunks, state : States, text : {0} \cup Versions]  \* pushes
  /\ asked \subseteq Articles                         \* the synced articles
  /\ replies \subseteq [art : Articles, text : Versions, chunks : SUBSET AllChunks]  \* sync answers

\* Nothing is written, nothing is on the peer, nothing is in flight.
Init ==
  /\ src = [a \in Articles |-> "none"]       \* no article yet
  /\ srcText = [a \in Articles |-> 0]        \* so no text
  /\ written = [a \in Articles |-> {}]       \* and no chunk
  /\ peerDoc = [a \in Articles |-> "none"]   \* the peer has none
  /\ peerText = [a \in Articles |-> 0]       \* nor any text
  /\ peerChunk = [c \in AllChunks |-> "none"]  \* nor any chunk
  /\ wire = {}                               \* no push in flight
  /\ asked = {}                              \* no sync asked for
  /\ replies = {}                            \* and none answered

\* Whether the filtered replicator is pushed this chunk.
ChunkSent(c) ==
  CASE ChunkRule = "parent" -> Owner(c) \in Matching  \* the fix: its article decides
    [] ChunkRule = "own" -> FALSE                      \* bug: a chunk never matches the filter
    [] ChunkRule = "all" -> TRUE                       \* bug: every chunk goes, matching or not

\* The pushes one write of article a sends: the article with its text v if
\* the filter matches it, and each chunk in cs the chunk rule sends, all in
\* state s. A chunk's push carries no text.
Pushes(a, s, v, cs) ==
  (IF a \in Matching THEN {[doc |-> a, state |-> s, text |-> v]} ELSE {})  \* the article
  \cup {[doc |-> c, state |-> s, text |-> 0] : c \in {k \in cs : ChunkSent(k)}}  \* its chunks

\* The source creates article a with its chunks in one transaction.
Create(a) ==
  /\ src[a] = "none"                         \* a is new
  /\ src' = [src EXCEPT ![a] = "live"]       \* a is live on the source
  /\ srcText' = [srcText EXCEPT ![a] = 1]    \* with the first text
  /\ written' = [written EXCEPT ![a] = Chunks(a, 1)]  \* and that text's chunks
  /\ wire' = wire \cup Pushes(a, "live", 1, Chunks(a, 1))  \* its pushes are sent
  /\ UNCHANGED <<peerDoc, peerText, peerChunk, asked, replies>>  \* the peer has not seen them yet

\* The source edits article a's text. The new text's chunks it lacks are
\* written in the same transaction; the old ones stay stored.
Edit(a) ==
  /\ src[a] = "live" /\ srcText[a] = 1        \* a is live with its first text
  /\ srcText' = [srcText EXCEPT ![a] = 2]     \* the edit writes the second text
  /\ written' = [written EXCEPT ![a] = @ \cup Chunks(a, 2)]  \* and stores its new chunk
  /\ wire' = wire \cup Pushes(a, "live", 2, Chunks(a, 2) \ written[a])  \* the new writes are pushed
  /\ UNCHANGED <<src, peerDoc, peerText, peerChunk, asked, replies>>  \* nothing else moves

\* The source deletes article a, and every chunk it stored, old ones too, in
\* the same transaction.
Delete(a) ==
  /\ src[a] = "live"                          \* a is live
  /\ src' = [src EXCEPT ![a] = "deleted"]     \* a is deleted on the source
  /\ wire' = wire \cup Pushes(a, "deleted", srcText[a], written[a])  \* the deletes are sent
  /\ UNCHANGED <<srcText, written, peerDoc, peerText, peerChunk, asked, replies>>  \* the peer has not seen them yet

\* One push reaches the peer, in any order.
Deliver(m) ==
  /\ m \in wire                               \* it was sent
  /\ wire' = wire \ {m}                       \* it is no longer in flight
  /\ IF m.doc \in Articles                    \* an article's push
     THEN /\ peerDoc' = [peerDoc EXCEPT ![m.doc] = Later(@, m.state)]  \* updates the article
          /\ peerText' = [peerText EXCEPT ![m.doc] = Max(@, m.text)]   \* and its text
          /\ UNCHANGED peerChunk              \* and no chunk
     ELSE /\ peerChunk' = [peerChunk EXCEPT ![m.doc] = Later(@, m.state)]  \* a chunk's push
          /\ UNCHANGED <<peerDoc, peerText>>  \* updates only that chunk
  /\ UNCHANGED <<src, srcText, written, asked, replies>>  \* the source does not move

\* The chunks a document sync of article a sends with it. The fix cuts the
\* current text and sends the chunks it uses, and a deleted article sends
\* none. One bug sends every chunk ever stored, another sends none.
SyncChunks(a) ==
  CASE SyncRule = "current" ->
         IF src[a] = "live" THEN Chunks(a, srcText[a]) ELSE {}  \* the fix
    [] SyncRule = "history" ->
         IF src[a] = "live" THEN written[a] ELSE {}             \* bug: the whole history
    [] SyncRule = "none" -> {}                                  \* bug: the article alone

\* The peer syncs article a by its ID. The source answers with the article's
\* heads and the heads of the chunks SyncChunks names.
Sync(a) ==
  /\ src[a] # "none"                          \* the source has the article
  /\ asked' = asked \cup {a}                  \* the peer asked for it
  /\ replies' = replies \cup {[art |-> a, text |-> srcText[a], chunks |-> SyncChunks(a)]}  \* the answer
  /\ peerDoc' = [peerDoc EXCEPT ![a] = Later(@, src[a])]  \* the article arrives
  /\ peerText' = [peerText EXCEPT ![a] = Max(@, srcText[a])]  \* with its text
  /\ peerChunk' = [c \in AllChunks |->        \* and the chunks sent arrive live
       IF c \in SyncChunks(a) THEN Later(peerChunk[c], "live") ELSE peerChunk[c]]
  /\ UNCHANGED <<src, srcText, written, wire>>  \* nothing new is pushed

\* Any of the steps above.
Next ==
  \/ \E a \in Articles : Create(a) \/ Edit(a) \/ Delete(a) \/ Sync(a)  \* the source writes or the peer syncs
  \/ \E m \in wire : Deliver(m)                                       \* or a push arrives

\* The whole behaviour.
Spec == Init /\ [][Next]_vars

\* Every push sent has arrived.
Quiet == wire = {}

\* Once quiet, a live article on the peer has every chunk of the text it
\* holds live with it.
INV_ChunksFollowParent ==
  Quiet => \A a \in Articles :
    peerDoc[a] = "live" => \A c \in Chunks(a, peerText[a]) : peerChunk[c] = "live"

\* A chunk reaches the peer only if its article matches the filter or the
\* peer asked for that article.
INV_NoForeignChunks ==
  \A c \in AllChunks :
    peerChunk[c] # "none" => Owner(c) \in Matching \cup asked

\* Once quiet, a matching article deleted on the peer leaves none of its
\* chunks live there.
INV_DeletesFollowParent ==
  Quiet => \A a \in Matching :
    peerDoc[a] = "deleted" => \A c \in AllChunks : Owner(c) = a => peerChunk[c] # "live"

\* Every sync answer sends only chunks of the article's text at that time, so
\* its size is bounded by that text, not by how often it was edited.
INV_SyncSendsCurrentChunks ==
  \A r \in replies : r.chunks \subseteq Chunks(r.art, r.text)
====

---- MODULE ChunkFollow ----
\* The story: a source node writes articles. Each article has a `@chunk` text,
\* so writing it also writes its chunk documents in a second collection. The
\* source can edit an article's text later. The edit writes the chunks of the
\* new text it lacks and deletes the old text's chunks the new one no longer
\* uses, in the same transaction. A peer gets articles two ways.
\* A replicator with a filter pushes it every write of an article the filter
\* matches. And the peer can ask for one article by its ID (a document sync).
\*
\* The filter names the article's own fields. A chunk has none of them, so
\* the push of a chunk must be decided by its article, and a document sync of
\* an article must bring the article's chunks with it. Only the chunks of the
\* article's current text come with a sync, so one reply never grows with the
\* article's history.
\*
\* An edit's chunk delete is pushed the way the chunk's create was, decided by
\* its article. A document sync never brings it, since it answers only with the
\* chunks the article still uses. So the peer's merge of an article also
\* deletes the chunks its text no longer uses, and builds the ones it lacks.
\* An encrypted text's merge does neither: there only the pushes and syncs
\* bring its chunks, and only the pushed delete cleans up.
\*
\* What must never happen:
\*   - the peer ends up with a live article but not all of the chunks of the
\*     text it holds (INV_ChunksFollowParent);
\*   - the peer holds a chunk of an article it was never sent nor asked for
\*     (INV_NoForeignChunks);
\*   - a matching article's delete reaches the peer and its chunks stay live
\*     there (INV_DeletesFollowParent);
\*   - a document sync answers with a chunk the article's current text does
\*     not use (INV_SyncSendsCurrentChunks);
\*   - once quiet, a live article on the peer has a chunk live that the text
\*     it holds does not use (INV_DroppedChunksGo).
\*
\* The peer's merge can build a chunk under an ID the source never used: a
\* chunk of the edit can reach the peer while it holds the first text, which
\* does not use it, so the peer deletes it, and the merge of the edit builds it
\* again under its next generation. No delete from the source names that ID, so the merge of
\* the article's delete deletes every chunk the peer holds for it.
\*
\* Code it models:
\*   crates/database/p2p/src/sync/coordinator/broadcast.rs  SyncCoordinator::push_jobs
\*     (a chunk's push is decided by its parent's filter)
\*   crates/database/db/src/merge/push_docs/follow.rs  replica_takes (replay and retry decide a
\*     chunk by its parent too)
\*   crates/database/db/src/chunk/sync/write.rs  ChunkWrites::publish, ChunkWrites::delete
\*     (each chunk head, create or delete, carries its parent)
\*   crates/database/db/src/chunk/sync/mod.rs  ChunkWrites::sync_chunks (an edit deletes
\*     the chunks its text no longer uses)
\*   crates/database/db/src/chunk/rebuild.rs  reconcile (a merge of the article deletes
\*     them too and builds the ones it lacks, unless its text is encrypted),
\*     remove_all (the merge of the article's delete deletes its chunks)
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
  SyncRule,     \* what a document sync sends with an article: "current" (the fix), "history" or "none"
  DeletesFollow, \* TRUE: an edit's chunk delete is pushed like the chunk's create (the fix)
  Reconcile,    \* TRUE: the peer's merge of an edit deletes the chunks its text stopped using (the fix)
  Cascade       \* TRUE: the peer's merge of the article's delete deletes the chunks it holds (the fix)

\* The filter matches only some of the articles.
ASSUME Matching \subseteq Articles
\* The three ways a chunk's push can be decided.
ASSUME ChunkRule \in {"parent", "own", "all"}
\* The three ways a document sync can answer.
ASSUME SyncRule \in {"current", "history", "none"}
\* The two cleanup switches are yes-or-no.
ASSUME DeletesFollow \in BOOLEAN /\ Reconcile \in BOOLEAN /\ Cascade \in BOOLEAN

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
  written,   \* written[a]: every chunk of a the source has ever stored, deleted ones too
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

\* The chunk rule's pushes of the chunks in cs alone, all in state s. A
\* chunk's push carries no text.
ChunkPushes(s, cs) == {[doc |-> c, state |-> s, text |-> 0] : c \in {k \in cs : ChunkSent(k)}}

\* The pushes one write of article a sends: the article with its text v if
\* the filter matches it, and each chunk in cs the chunk rule sends, all in
\* state s. A chunk's push carries no text.
Pushes(a, s, v, cs) ==
  (IF a \in Matching THEN {[doc |-> a, state |-> s, text |-> v]} ELSE {})  \* the article
  \cup ChunkPushes(s, cs)                                                     \* and its chunks

\* The chunks the source stores for article a: those of its current text
\* while it is live, since an edit deletes the ones the new text drops.
Stored(a) == IF src[a] = "live" THEN Chunks(a, srcText[a]) ELSE {}

\* The peer's chunks once its article a is in state s with text v. With the
\* fix, a live article's merge deletes each live chunk of a that v does not
\* use, and builds each chunk of v the peer lacks, a deleted one under a new
\* ID. With the cascade, the merge of a's delete deletes every chunk of a the
\* peer holds, the ones only the peer built too.
Reconciled(a, s, v, chunks) ==
  [c \in AllChunks |->
     IF Owner(c) # a THEN chunks[c]                                  \* another article's chunk stays as it is
     ELSE IF s = "deleted" THEN                                      \* the article's delete merged
       IF Cascade /\ chunks[c] = "live" THEN "deleted" ELSE chunks[c] \* deletes its live chunks with the cascade
     ELSE IF ~Reconcile \/ s # "live" THEN chunks[c]                 \* only a live article's merge cleans up
     ELSE IF c \in Chunks(a, v) THEN "live"                          \* a chunk the text uses is live, built if missing
     ELSE IF chunks[c] = "live" THEN "deleted"                       \* a live one it does not use is deleted
     ELSE chunks[c]]                                                 \* and the rest stay as they are

\* The source creates article a with its chunks in one transaction.
Create(a) ==
  /\ src[a] = "none"                         \* a is new
  /\ src' = [src EXCEPT ![a] = "live"]       \* a is live on the source
  /\ srcText' = [srcText EXCEPT ![a] = 1]    \* with the first text
  /\ written' = [written EXCEPT ![a] = Chunks(a, 1)]  \* and that text's chunks
  /\ wire' = wire \cup Pushes(a, "live", 1, Chunks(a, 1))  \* its pushes are sent
  /\ UNCHANGED <<peerDoc, peerText, peerChunk, asked, replies>>  \* the peer has not seen them yet

\* The source edits article a's text. In the same transaction the new text's
\* chunks it lacks are written and the old text's chunks it drops are deleted.
Edit(a) ==
  /\ src[a] = "live" /\ srcText[a] = 1        \* a is live with its first text
  /\ srcText' = [srcText EXCEPT ![a] = 2]     \* the edit writes the second text
  /\ written' = [written EXCEPT ![a] = @ \cup Chunks(a, 2)]  \* and stores its new chunk
  /\ wire' = wire \cup Pushes(a, "live", 2, Chunks(a, 2) \ written[a])  \* the new writes are pushed,
           \cup (IF DeletesFollow                                    \* and with the fix the deletes of
              THEN ChunkPushes("deleted", Chunks(a, 1) \ Chunks(a, 2))  \* the dropped chunks, as their creates were
              ELSE {})                                                \* or without it, no delete at all
  /\ UNCHANGED <<src, peerDoc, peerText, peerChunk, asked, replies>>  \* nothing else moves

\* The source deletes article a, and every chunk it still stores, in the same
\* transaction.
Delete(a) ==
  /\ src[a] = "live"                          \* a is live
  /\ src' = [src EXCEPT ![a] = "deleted"]     \* a is deleted on the source
  /\ wire' = wire \cup Pushes(a, "deleted", srcText[a], Stored(a))  \* the deletes are sent
  /\ UNCHANGED <<srcText, written, peerDoc, peerText, peerChunk, asked, replies>>  \* the peer has not seen them yet

\* Whether a chunk delete of c must wait on the peer: its article has not
\* arrived, or is live and its text uses c. ChunkMerge.tla checks this guard.
Waits(c) ==
  \/ peerDoc[Owner(c)] = "none"                                               \* the article is not here
  \/ peerDoc[Owner(c)] = "live" /\ c \in Chunks(Owner(c), peerText[Owner(c)])  \* or uses c

\* One push reaches the peer, in any order. A chunk delete that must wait
\* stays on its way.
Deliver(m) ==
  /\ m \in wire                               \* it was sent
  /\ (m.doc \notin Articles /\ m.state = "deleted") => ~Waits(m.doc)  \* a chunk delete merges only when it may
  /\ wire' = wire \ {m}                       \* it is no longer in flight
  /\ IF m.doc \in Articles                    \* an article's push
     THEN LET s == Later(peerDoc[m.doc], m.state)  \* the article's new state
              v == Max(peerText[m.doc], m.text)    \* and text
          IN /\ peerDoc' = [peerDoc EXCEPT ![m.doc] = s]   \* updates the article
             /\ peerText' = [peerText EXCEPT ![m.doc] = v]  \* and its text
             /\ peerChunk' = Reconciled(m.doc, s, v, peerChunk)  \* and its merge cleans up
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
  /\ LET s == Later(peerDoc[a], src[a])        \* the article's new state on the peer
         v == Max(peerText[a], srcText[a])     \* and its text
         got == [c \in AllChunks |->           \* the chunks sent arrive live
                   IF c \in SyncChunks(a) THEN Later(peerChunk[c], "live") ELSE peerChunk[c]]
     IN /\ peerDoc' = [peerDoc EXCEPT ![a] = s]   \* the article arrives
        /\ peerText' = [peerText EXCEPT ![a] = v]  \* with its text
        /\ peerChunk' = Reconciled(a, s, v, got)   \* and its chunks, cleaned up
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

\* Once quiet, a live article on the peer has no chunk live that the text it
\* holds does not use.
INV_DroppedChunksGo ==
  Quiet => \A a \in Articles :
    peerDoc[a] = "live" =>                                        \* for a live article on the peer,
      \A c \in AllChunks :                                       \* each chunk
        (Owner(c) = a /\ c \notin Chunks(a, peerText[a])) => peerChunk[c] # "live"  \* of it the text drops is not live
====

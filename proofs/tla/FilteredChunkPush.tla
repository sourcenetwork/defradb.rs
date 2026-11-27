---- MODULE FilteredChunkPush ----
\* The story: a node (the sender) holds articles whose text is `@chunk`, so
\* each article has chunk documents of its own. Here every article has one
\* chunk, which stands for all of them. A replicator sends a second node (the
\* receiver) only the articles its filter matches. The filter may only name
\* `@immutable` fields, so whether an article matches never changes; only
\* adding the replicator again with a new filter changes what matches.
\*
\* The rule: the receiver gets a chunk only together with its article, or
\* after it. A chunk is decided by its article's document and filter, never by
\* its own document, which has none of the article's fields.
\*
\* What can happen:
\*   - the sender writes an article and its chunk in one transaction, as a
\*     single write or in an explicit transaction; the live push sends both;
\*   - an article or a chunk arrives from another node; forwarding it, the
\*     sender has no article document at hand, so a filtered peer gets nothing;
\*   - the replicator is added and the replay sends what exists, each chunk
\*     after its article; an article's push can fail to build, as when the CAR
\*     authority is full or signing fails, and then it waits for its retry;
\*   - some articles' heads can't be read here, so neither the replay nor a
\*     retry can send them;
\*   - the replicator is added again with a filter that matches more, and the
\*     replay sends the newly matching articles and their chunks;
\*   - a push is lost and the retry sends it again; a chunk's retry sends its
\*     article with it, or ends when the article is not here or can't be read;
\*   - a chunk marker may be left from before the filter, for a chunk whose
\*     article arrives only later.
\*
\* What must never happen:
\*   - the receiver gets a document of an article no filter matched
\*     (INV_NothingUnmatched);
\*   - the receiver gets a chunk while its article was never sent to it
\*     (INV_ChunkOnlyWithItsArticle), unless the article's own push was lost
\*     and its heads can't be read for a retry;
\*   - a retry that never ends (LIVE_RetriesEnd): a failing retry holds the
\*     peer's backoff up and marks it inactive on every pass;
\*   - a matching article written here, whose heads can be read, reaches the
\*     receiver and its chunk never follows (LIVE_ChunkFollowsItsArticle). A chunk that arrives from
\*     another node after its article was sent is forwarded with no document,
\*     so it is not covered: that is the relay limit;
\*   - a replay looks at a matching chunk whose article can be read, and the
\*     chunk never reaches the receiver (LIVE_ReplayShipsChunks).
\*
\* Code it models:
\*   crates/database/p2p/src/replicator.rs  ReplicatorInfo::admission (the one rule every
\*     push path decides by), is_filtered_for_collection, remove_collection
\*   crates/database/p2p/src/sync/coordinator/broadcast.rs  SyncCoordinator::push_jobs,
\*     push_followed_document_to_replicators_with_creator
\*   crates/surfaces/p2p-adapter/src/lib.rs  set_chunk_sources (at install and restore),
\*     collections_requiring_replay (a changed parent filter replays its chunks)
\*   crates/database/db/src/merge/broadcast_mutator/mod.rs  BroadcastMutator::derived_heads
\*   crates/database/db/src/merge/broadcast_mutator/batch.rs  BroadcastBatchMutator::capture_derived
\*   crates/database/db/src/write/doc.rs  DbDocMutator (an explicit transaction's FollowedParent)
\*   crates/database/db/src/merge/txn_broadcaster.rs  broadcast (explicit transaction)
\*   crates/database/db/src/merge/push_docs/follow.rs  ReplicaFilter::decide
\*   crates/database/db/src/merge/push_docs.rs  push_existing_docs_owned (replay: parents
\*     first, queued_parents, deferred_chunks), retry_doc, send_parent_first
\*   crates/database/db/src/merge/replication_ack.rs  acknowledged_heads (no head is no delivery)
\* Finite sets are all this model needs.
EXTENDS FiniteSets

CONSTANTS
  Articles,           \* the articles, for example {a1, a2}
  Matches,            \* the articles the first filter matches, for example {a1}
  NewMatches,         \* the articles the filter of a re-add matches: all of Matches and maybe more
  Unreadable,         \* articles whose heads can't be read on the sender
  StaleMarkers,       \* articles whose chunk has a marker from before the filter
  ShipChunks,         \* TRUE: a filtered article's chunks are replicated at all (the fix)
  MarkChunksFiltered, \* TRUE: the chunk collection counts as filtered with its article (the fix)
  DecideByArticle,    \* TRUE: a chunk is decided by its article's document (the fix)
  RequireArticle,     \* TRUE: a chunk with no article document at hand is not sent (the fix)
  TxnCarriesArticle,  \* TRUE: an explicit transaction hands its chunks the article's document (the fix)
  RetrySendsArticle,  \* TRUE: a chunk's retry sends its article with it (the fix)
  RetryEnds,          \* TRUE: a chunk retry whose article is not here ends (the fix)
  RetryNeedsReadableArticle, \* TRUE: a chunk retry whose article has no head to send ends unsent (the fix)
  ReplayNeedsArticle, \* TRUE: the replay sends a chunk only after queueing its article (the fix)
  ReplayDefersChunk,  \* TRUE: a chunk whose matching article failed to queue gets a retry marker (the fix)
  ReplayChunksOnRefilter \* TRUE: a new article filter replays the chunk collection too (the fix)

\* A re-add only widens the filter, within the articles.
ASSUME Matches \subseteq NewMatches /\ NewMatches \subseteq Articles
\* The unreadable and stale articles are some of the articles.
ASSUME Unreadable \subseteq Articles /\ StaleMarkers \subseteq Articles
\* Each switch is yes-or-no.
ASSUME ShipChunks \in BOOLEAN /\ MarkChunksFiltered \in BOOLEAN /\ DecideByArticle \in BOOLEAN
\* More switches, each yes-or-no.
ASSUME RequireArticle \in BOOLEAN /\ TxnCarriesArticle \in BOOLEAN
\* The retry switches, each yes-or-no.
ASSUME RetrySendsArticle \in BOOLEAN /\ RetryEnds \in BOOLEAN /\ RetryNeedsReadableArticle \in BOOLEAN
\* The replay switches, each yes-or-no.
ASSUME ReplayNeedsArticle \in BOOLEAN /\ ReplayDefersChunk \in BOOLEAN /\ ReplayChunksOnRefilter \in BOOLEAN

\* The states a document can be in: not there, there, or deleted.
States == {"none", "live", "deleted"}
\* The two documents of an article that travel: the article and its chunk.
Kinds == {"article", "chunk"}
\* A push names an article and which of its documents it sends.
Items == Articles \X Kinds

VARIABLES
  sent,       \* sent[a][k]: the state of article a's document k on the sender
  recv,       \* recv[a][k]: the state of article a's document k on the receiver
  added,      \* TRUE once the replicator is added
  matching,   \* the articles the replicator's filter matches now
  refiltered, \* TRUE once the replicator was added again with the new filter
  inflight,   \* pushes on their way to the receiver
  retry,      \* pushes that were lost (or left from before) and wait for the retry
  replayLeft, \* documents the replay still has to look at
  queued,     \* articles the replay has queued, which their chunks may follow
  replayed,   \* readable matching articles whose chunk a replay looked at
  local,      \* the articles written on the sender, not forwarded from elsewhere
  lost        \* unreadable articles whose own push was lost, which no retry can send

\* All the variables, for stuttering steps.
vars == <<sent, recv, added, matching, refiltered, inflight, retry, replayLeft, queued, replayed, local, lost>>

\* The type of every variable.
TypeOK ==
  /\ sent \in [Articles -> [Kinds -> States]]  \* each document on the sender has a state
  /\ recv \in [Articles -> [Kinds -> States]]  \* each document on the receiver has a state
  /\ added \in BOOLEAN                        \* the replicator is added or not
  /\ matching \in {Matches, NewMatches}       \* the first filter or the new one
  /\ refiltered \in BOOLEAN                   \* re-added or not
  /\ inflight \subseteq Items                 \* pushes on their way
  /\ retry \subseteq Items                    \* at most one marker per document
  /\ replayLeft \subseteq Items               \* the replay's work
  /\ queued \subseteq Articles                \* the articles the replay queued
  /\ replayed \subseteq Articles              \* the replay's chunks that must arrive
  /\ local \subseteq Articles                 \* local writes are of articles
  /\ lost \subseteq Unreadable                \* only an unreadable article is lost

\* Nothing exists; only the stale chunk markers wait.
Init ==
  /\ sent = [a \in Articles |-> [k \in Kinds |-> "none"]] \* nothing on the sender
  /\ recv = [a \in Articles |-> [k \in Kinds |-> "none"]] \* nothing on the receiver
  /\ added = FALSE                                      \* no replicator yet
  /\ matching = Matches                                 \* the first filter
  /\ refiltered = FALSE                                 \* not re-added yet
  /\ inflight = {}                                      \* nothing on its way
  /\ retry = StaleMarkers \X {"chunk"}                  \* markers left from before the filter
  /\ replayLeft = {}                                    \* no replay running
  /\ queued = {}                                        \* nothing queued
  /\ replayed = {}                                      \* no replay yet
  /\ local = {}                                         \* nothing written yet
  /\ lost = {}                                          \* nothing lost yet

\* The sender holds article a itself, so its document is at hand.
Here(a) == sent[a]["article"] # "none"

\* The document at hand for article a: "article" when it is here, "none" otherwise.
DocOf(a) == IF Here(a) THEN "article" ELSE "none"

\* Whether a push of a's chunk goes, given the document `doc` at hand.
AdmitsChunk(a, doc) ==
  IF ~ShipChunks THEN FALSE                 \* the old code left the chunks out
  ELSE IF ~MarkChunksFiltered THEN TRUE     \* bug: the chunk counts as unfiltered and always goes
  ELSE IF ~DecideByArticle THEN TRUE        \* bug: the chunk's own document passes the filter
  ELSE IF doc = "none" THEN ~RequireArticle  \* no article document: fail closed, unless the bug lets it through
  ELSE a \in matching                       \* the article's document decides, with its filter

\* Whether a push of article a goes: only with its document, when it matches.
AdmitsArticle(a, doc) == doc = "article" /\ a \in matching

\* The pushes a write of a queues: the article and, with `chunkDoc` at hand, its chunk.
WritePushes(a, chunkDoc) ==
  (IF added /\ AdmitsArticle(a, "article") THEN {<<a, "article">>} ELSE {})  \* the article
  \cup (IF added /\ AdmitsChunk(a, chunkDoc) THEN {<<a, "chunk">>} ELSE {})  \* its chunk

\* The document a write on path `path` hands its chunk.
ChunkDoc(path) == IF path = "txn" /\ ~TxnCarriesArticle THEN "none" ELSE "article"

\* The two ways a write commits: a single write, or an explicit transaction.
Paths == {"single", "txn"}

\* The sender creates article a and its chunk in one transaction.
Create(a, path) ==
  /\ sent[a]["article"] = "none" /\ sent[a]["chunk"] = "none"  \* a does not exist here
  /\ sent' = [sent EXCEPT ![a] = [k \in Kinds |-> "live"]]      \* both are written
  /\ local' = local \cup {a}                                    \* by this node
  /\ inflight' = inflight \cup WritePushes(a, ChunkDoc(path))   \* the live push
  /\ UNCHANGED <<recv, added, matching, refiltered, retry, replayLeft, queued, replayed, lost>> \* nothing else moves

\* The sender deletes article a and its chunk in one transaction.
Delete(a, path) ==
  /\ sent[a]["article"] = "live" /\ sent[a]["chunk"] = "live"  \* a is live here
  /\ sent' = [sent EXCEPT ![a] = [k \in Kinds |-> "deleted"]]   \* both are deleted
  /\ inflight' = inflight \cup WritePushes(a, ChunkDoc(path))   \* the live push
  /\ UNCHANGED <<recv, added, matching, refiltered, retry, replayLeft, queued, replayed, local, lost>> \* nothing else moves

\* Article a's document k arrives from another node and is forwarded on,
\* with no article document at hand. A chunk can come before its article.
Forward(a, k) ==
  /\ sent[a][k] = "none"                                    \* it is not here yet
  /\ sent' = [sent EXCEPT ![a][k] = "live"]                 \* it merges here
  /\ inflight' = IF k = "chunk" /\ added /\ AdmitsChunk(a, "none")
                 THEN inflight \cup {<<a, "chunk">>}        \* a chunk forward with no document
                 ELSE inflight                              \* an article forward never goes
  /\ UNCHANGED <<recv, added, matching, refiltered, retry, replayLeft, queued, replayed, local, lost>> \* nothing else moves

\* The documents of `kinds` the sender holds, for a replay to look at.
HeldOf(kinds) == {i \in Articles \X kinds : sent[i[1]][i[2]] # "none"}

\* The readable articles of `filter` whose chunk a replay of `kinds` looks at,
\* with their article here to decide them.
ReplayedChunks(kinds, filter) ==
  IF "chunk" \in kinds                                       \* only a replay of the chunks
  THEN {a \in filter \ Unreadable : sent[a]["chunk"] # "none" /\ Here(a)}
  ELSE {}                                                   \* otherwise none

\* The replicator is added; the replay will look at every document here.
AddReplicator ==
  /\ ~added                                                 \* not added yet
  /\ added' = TRUE                                          \* now it is
  /\ replayLeft' = HeldOf(Kinds)                            \* both collections replay
  /\ replayed' = replayed \cup ReplayedChunks(Kinds, matching) \* these chunks must arrive
  /\ UNCHANGED <<sent, recv, matching, refiltered, inflight, retry, queued, local, lost>> \* nothing else moves

\* The replicator is added again with the new filter, once the first replay
\* is done. The article collection's filter changed, so it replays; with the
\* fix the chunk collection does too, since its article's filter decides it.
Refilter ==
  /\ added /\ ~refiltered /\ replayLeft = {}                \* after the first replay
  /\ NewMatches # Matches                                   \* a re-add with the same filter replays nothing
  /\ refiltered' = TRUE                                     \* now re-added
  /\ matching' = NewMatches                                 \* with the new filter
  /\ LET kinds == IF ReplayChunksOnRefilter THEN Kinds ELSE {"article"} \* the bug skips the chunks
     IN /\ replayLeft' = HeldOf(kinds)                      \* the changed collections replay
        /\ replayed' = replayed \cup ReplayedChunks(kinds, NewMatches) \* these chunks must arrive
  /\ queued' = {}                                           \* a new replay queues afresh
  /\ UNCHANGED <<sent, recv, added, inflight, retry, local, lost>> \* nothing else moves

\* The replay looks at a's article. A matching one is queued and sent, or its
\* push fails to build and it waits for its retry. One whose heads can't be
\* read is not sent at all.
ReplayArticle(a) ==
  /\ <<a, "article">> \in replayLeft                          \* a's article is still to be looked at
  /\ replayLeft' = replayLeft \ {<<a, "article">>}            \* and now it is
  /\ IF AdmitsArticle(a, DocOf(a)) /\ a \notin Unreadable     \* when it matches and can be read,
     THEN \/ /\ inflight' = inflight \cup {<<a, "article">>}   \* it is sent
             /\ queued' = queued \cup {a}                      \* and its chunks may follow
             /\ UNCHANGED retry                                \* with no marker left
          \/ /\ retry' = retry \cup {<<a, "article">>}         \* or its push fails and waits
             /\ UNCHANGED <<inflight, queued>>                 \* and is not queued
     ELSE UNCHANGED <<inflight, queued, retry>>               \* otherwise nothing
  /\ UNCHANGED <<sent, recv, added, matching, refiltered, replayed, local, lost>> \* nothing else moves

\* The replay looks at a's chunk after its article's turn, decided by the
\* article. It goes when its article was queued in this replay; a chunk of a
\* matching article that was not queued waits for its retry, which sends both.
ReplayChunk(a) ==
  /\ <<a, "chunk">> \in replayLeft                            \* a's chunk is still to be looked at
  /\ <<a, "article">> \notin replayLeft                        \* articles go first
  /\ replayLeft' = replayLeft \ {<<a, "chunk">>}              \* and now it is
  /\ LET admitted == AdmitsChunk(a, DocOf(a))                  \* the article's filter lets it go
         go == admitted /\ (a \in queued \/ ~ReplayNeedsArticle) \* the bug sends it anyway
         defer == admitted /\ ~go /\ ReplayDefersChunk         \* its article failed to queue
     IN /\ inflight' = IF go THEN inflight \cup {<<a, "chunk">>} ELSE inflight \* sent behind its article
        /\ retry' = IF defer THEN retry \cup {<<a, "chunk">>} ELSE retry       \* or left to its retry
  /\ UNCHANGED <<sent, recv, added, matching, refiltered, queued, replayed, local, lost>> \* nothing else moves

\* A push arrives: the receiver pulls the document's current head.
Deliver(i) ==
  /\ i \in inflight                                         \* i is on its way
  /\ recv' = [recv EXCEPT ![i[1]][i[2]] = sent[i[1]][i[2]]]  \* the receiver gets the sender's state
  /\ inflight' = inflight \ {i}                             \* the push is done
  /\ UNCHANGED <<sent, added, matching, refiltered, retry, replayLeft, queued, replayed, local, lost>> \* nothing else moves

\* A push is lost; its marker stays for the retry. The live push of an
\* article whose heads can't be read is the one push no retry can send again.
Lose(i) ==
  /\ i \in inflight                                         \* i is on its way
  /\ inflight' = inflight \ {i}                             \* and gets lost
  /\ retry' = retry \cup {i}                                \* the retry owes it
  /\ lost' = IF i[2] = "article" /\ i[1] \in Unreadable      \* an unreadable article's own push
            THEN lost \cup {i[1]} ELSE lost                 \* is lost for good
  /\ UNCHANGED <<sent, recv, added, matching, refiltered, replayLeft, queued, replayed, local>> \* nothing else moves

\* The retry of a's article: sent when here, readable and matching; the marker clears.
RetryArticle(a) ==
  /\ added /\ <<a, "article">> \in retry                    \* it is owed
  /\ retry' = retry \ {<<a, "article">>}                    \* the marker clears
  /\ recv' = IF AdmitsArticle(a, DocOf(a)) /\ a \notin Unreadable \* sent if it matches and can be read,
             THEN [recv EXCEPT ![a]["article"] = sent[a]["article"]]
             ELSE recv                                      \* otherwise not at all
  /\ UNCHANGED <<sent, added, matching, refiltered, inflight, replayLeft, queued, replayed, local, lost>> \* nothing else moves

\* The retry of a's chunk, decided by its article. A passing chunk is sent
\* together with its article. Without the article here the marker clears;
\* the bug instead fails the retry and keeps the marker, so nothing changes.
\* An article with no head to send ends the retry unsent; the bug sends the
\* chunk alone.
RetryChunk(a) ==
  /\ added /\ <<a, "chunk">> \in retry                      \* it is owed
  /\ ~(~Here(a) /\ RequireArticle /\ ~RetryEnds)            \* the bug's failing retry changes nothing
  /\ retry' = retry \ {<<a, "chunk">>}                      \* the marker clears
  /\ LET readable == a \notin Unreadable                     \* the article's heads can be sent
         go == /\ sent[a]["chunk"] # "none"                  \* a chunk not here is not sent
               /\ AdmitsChunk(a, DocOf(a))                   \* its article's filter lets it go
               /\ (readable \/ ~RetryNeedsReadableArticle)   \* and its article can go first
         withArticle == go /\ RetrySendsArticle /\ Here(a) /\ readable \* its article with it
     IN recv' = [recv EXCEPT
                   ![a]["chunk"] = IF go THEN sent[a]["chunk"] ELSE @,                 \* the chunk
                   ![a]["article"] = IF withArticle THEN sent[a]["article"] ELSE @]    \* its article
  /\ UNCHANGED <<sent, added, matching, refiltered, inflight, replayLeft, queued, replayed, local, lost>> \* nothing else moves

\* Every step the system can take.
Next ==
  \/ \E a \in Articles, p \in Paths : Create(a, p) \/ Delete(a, p) \* writes on either path
  \/ \E a \in Articles, k \in Kinds : Forward(a, k)                \* forwarded documents
  \/ AddReplicator \/ Refilter                                     \* the replicator is set up
  \/ \E a \in Articles : ReplayArticle(a) \/ ReplayChunk(a)        \* the replay
  \/ \E a \in Articles : RetryArticle(a) \/ RetryChunk(a)          \* the retries
  \/ \E i \in Items : Deliver(i) \/ Lose(i)                        \* pushes arrive or get lost

\* A push on its way is eventually delivered or lost, and the replicator, the
\* replay and every retry that can run eventually do. A re-add may never come.
Fairness ==
  /\ WF_vars(AddReplicator)                                  \* the replicator is added
  /\ \A i \in Items : WF_vars(Deliver(i) \/ Lose(i))           \* every push ends
  /\ \A a \in Articles : WF_vars(ReplayArticle(a)) /\ WF_vars(ReplayChunk(a))  \* the replay runs
  /\ \A a \in Articles : WF_vars(RetryArticle(a)) /\ WF_vars(RetryChunk(a))    \* the retries run

\* Start from Init and take any steps, fairly.
Spec == Init /\ [][Next]_vars /\ Fairness

\* The receiver never holds a document of an article no filter matched.
INV_NothingUnmatched ==
  \A a \in Articles \ NewMatches : \A k \in Kinds : recv[a][k] = "none"

\* The receiver holds a chunk only when its article reached it or is still
\* owed to it. The one exception is the article's own failure: its push was
\* lost and its heads can't be read for the retry, which no chunk rule mends.
INV_ChunkOnlyWithItsArticle ==
  \A a \in Articles :
    recv[a]["chunk"] # "none" =>                            \* a chunk on the receiver
      \/ recv[a]["article"] # "none"                        \* has its article there
      \/ <<a, "article">> \in inflight \cup retry           \* or on its way
      \/ a \in lost                                         \* or its article failed on its own

\* Every retry eventually ends: none stays failing forever.
LIVE_RetriesEnd == <>[](retry = {})

\* Once a matching article written here reached the receiver, its chunk
\* follows, unless the article's heads can't be read here for its retry.
LIVE_ChunkFollowsItsArticle ==
  \A a \in NewMatches \ Unreadable :
    (a \in local /\ recv[a]["article"] = "live" /\ sent[a]["chunk"] = "live")
      ~> (recv[a]["chunk"] # "none")

\* A matching chunk a replay looked at, whose article can be read, reaches the receiver.
LIVE_ReplayShipsChunks ==
  \A a \in Articles : (a \in replayed) ~> (recv[a]["chunk"] # "none")
====

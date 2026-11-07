---- MODULE FilteredChunkPush ----
\* The story: a node (the sender) holds articles whose text is `@chunk`, so
\* each article has chunk documents of its own. Here every article has one
\* chunk, which stands for all of them. A replicator sends a second node (the
\* receiver) only the articles its filter matches. The filter may only name
\* `@immutable` fields, so whether an article matches never changes.
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
\*     after its article;
\*   - a push is lost and the retry sends it again; a chunk's retry sends its
\*     article with it, or ends when the article is not here;
\*   - a chunk marker may be left from before the filter, for a chunk whose
\*     article arrives only later.
\*
\* What must never happen:
\*   - the receiver gets a document of an article the filter does not match
\*     (INV_NothingUnmatched);
\*   - the receiver gets a chunk while its article was never sent to it
\*     (INV_ChunkOnlyWithItsArticle);
\*   - a retry that never ends (LIVE_RetriesEnd): a failing retry holds the
\*     peer's backoff up and marks it inactive on every pass;
\*   - a matching article written here reaches the receiver and its chunk
\*     never follows (LIVE_ChunkFollowsItsArticle). A chunk that arrives from
\*     another node after its article was sent is forwarded with no document,
\*     so it is not covered: that is the relay limit.
\*
\* Code it models:
\*   crates/p2p/src/replicator.rs  ReplicatorInfo::is_filtered_for_collection,
\*     ReplicatorInfo::matches_chunk_source, ReplicatorInfo::matches_filter
\*   crates/p2p/src/sync/coordinator/broadcast.rs  push_jobs, PendingPush::admission
\*   crates/p2p-adapter/src/lib.rs  with_chunk_collections, chunk_source_ids
\*   crates/db/src/merge/broadcast_mutator/mod.rs  BroadcastMutator::publish_derived
\*   crates/db/src/merge/broadcast_mutator/batch.rs  BroadcastBatchMutator::capture_derived
\*   crates/db/src/chunk/sync/write.rs  ChunkWrites::publish (explicit transaction)
\*   crates/db/src/merge/txn_broadcaster.rs  broadcast (explicit transaction)
\*   crates/db/src/merge/push_filter.rs  FilterCheck::decide
\*   crates/db/src/merge/push_docs.rs  push_existing_docs (replay), retry_doc
\* Finite sets are all this model needs.
EXTENDS FiniteSets

CONSTANTS
  Articles,           \* the articles, for example {a1, a2}
  Matches,            \* the articles the filter matches, for example {a1}
  StaleMarkers,       \* articles whose chunk has a marker from before the filter
  ShipChunks,         \* TRUE: a filtered article's chunks are replicated at all (the fix)
  MarkChunksFiltered, \* TRUE: the chunk collection counts as filtered with its article (the fix)
  DecideByArticle,    \* TRUE: a chunk is decided by its article's document (the fix)
  RequireArticle,     \* TRUE: a chunk with no article document at hand is not sent (the fix)
  TxnCarriesArticle,  \* TRUE: an explicit transaction hands its chunks the article's document (the fix)
  RetrySendsArticle,  \* TRUE: a chunk's retry sends its article with it (the fix)
  RetryEnds           \* TRUE: a chunk retry whose article is not here ends (the fix)

\* The matching and stale articles are some of the articles.
ASSUME Matches \subseteq Articles /\ StaleMarkers \subseteq Articles
\* Each switch is yes-or-no.
ASSUME ShipChunks \in BOOLEAN /\ MarkChunksFiltered \in BOOLEAN /\ DecideByArticle \in BOOLEAN
ASSUME RequireArticle \in BOOLEAN /\ TxnCarriesArticle \in BOOLEAN
ASSUME RetrySendsArticle \in BOOLEAN /\ RetryEnds \in BOOLEAN

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
  inflight,   \* pushes on their way to the receiver
  retry,      \* pushes that were lost (or left from before) and wait for the retry
  replayLeft, \* articles the replay still has to look at
  local       \* the articles written on the sender, not forwarded from elsewhere

\* All the variables, for stuttering steps.
vars == <<sent, recv, added, inflight, retry, replayLeft, local>>

\* The type of every variable.
TypeOK ==
  /\ sent \in [Articles -> [Kinds -> States]]  \* each document on the sender has a state
  /\ recv \in [Articles -> [Kinds -> States]]  \* each document on the receiver has a state
  /\ added \in BOOLEAN                        \* the replicator is added or not
  /\ inflight \subseteq Items                 \* pushes on their way
  /\ retry \subseteq Items                    \* at most one marker per document
  /\ replayLeft \subseteq Articles            \* the replay's work
  /\ local \subseteq Articles                 \* local writes are of articles

\* Nothing exists; only the stale chunk markers wait.
Init ==
  /\ sent = [a \in Articles |-> [k \in Kinds |-> "none"]] \* nothing on the sender
  /\ recv = [a \in Articles |-> [k \in Kinds |-> "none"]] \* nothing on the receiver
  /\ added = FALSE                                      \* no replicator yet
  /\ inflight = {}                                      \* nothing on its way
  /\ retry = StaleMarkers \X {"chunk"}                  \* markers left from before the filter
  /\ replayLeft = {}                                    \* no replay running
  /\ local = {}                                         \* nothing written yet

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
  ELSE a \in Matches                        \* the article's document decides, with its filter

\* Whether a push of article a goes: only with its document, when it matches.
AdmitsArticle(a, doc) == doc = "article" /\ a \in Matches

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
  /\ UNCHANGED <<recv, added, retry, replayLeft>>               \* nothing else moves

\* The sender deletes article a and its chunk in one transaction.
Delete(a, path) ==
  /\ sent[a]["article"] = "live" /\ sent[a]["chunk"] = "live"  \* a is live here
  /\ sent' = [sent EXCEPT ![a] = [k \in Kinds |-> "deleted"]]   \* both are deleted
  /\ inflight' = inflight \cup WritePushes(a, ChunkDoc(path))   \* the live push
  /\ UNCHANGED <<recv, added, retry, replayLeft, local>>               \* nothing else moves

\* Article a's document k arrives from another node and is forwarded on,
\* with no article document at hand. A chunk can come before its article.
Forward(a, k) ==
  /\ sent[a][k] = "none"                                    \* it is not here yet
  /\ sent' = [sent EXCEPT ![a][k] = "live"]                 \* it merges here
  /\ inflight' = IF k = "chunk" /\ added /\ AdmitsChunk(a, "none")
                 THEN inflight \cup {<<a, "chunk">>}        \* a chunk forward with no document
                 ELSE inflight                              \* an article forward never goes
  /\ UNCHANGED <<recv, added, retry, replayLeft, local>>           \* nothing else moves

\* The replicator is added; the replay will look at every article with something here.
AddReplicator ==
  /\ ~added                                                 \* not added yet
  /\ added' = TRUE                                          \* now it is
  /\ replayLeft' = {a \in Articles : \E k \in Kinds : sent[a][k] # "none"}
  /\ UNCHANGED <<sent, recv, inflight, retry, local>>              \* nothing else moves

\* The replay queues a's article when it matches, and its chunk only after its queued article.
Replay(a) ==
  /\ a \in replayLeft                                       \* a is still to be looked at
  /\ replayLeft' = replayLeft \ {a}                         \* and now it is
  /\ LET art == IF AdmitsArticle(a, DocOf(a)) THEN {<<a, "article">>} ELSE {} \* the article
         chk == IF sent[a]["chunk"] # "none" /\ AdmitsChunk(a, DocOf(a))
                   /\ (art # {} \/ ~RequireArticle)
                THEN {<<a, "chunk">>} ELSE {}                \* the chunk, behind its article
     IN inflight' = inflight \cup art \cup chk
  /\ UNCHANGED <<sent, recv, added, retry, local>>                 \* nothing else moves

\* A push arrives: the receiver pulls the document's current head.
Deliver(i) ==
  /\ i \in inflight                                         \* i is on its way
  /\ recv' = [recv EXCEPT ![i[1]][i[2]] = sent[i[1]][i[2]]]  \* the receiver gets the sender's state
  /\ inflight' = inflight \ {i}                             \* the push is done
  /\ UNCHANGED <<sent, added, retry, replayLeft, local>>           \* nothing else moves

\* A push is lost; its marker stays for the retry.
Lose(i) ==
  /\ i \in inflight                                         \* i is on its way
  /\ inflight' = inflight \ {i}                             \* and gets lost
  /\ retry' = retry \cup {i}                                \* the retry owes it
  /\ UNCHANGED <<sent, recv, added, replayLeft, local>>            \* nothing else moves

\* The retry of a's article: sent when here and matching, and the marker clears.
RetryArticle(a) ==
  /\ added /\ <<a, "article">> \in retry                    \* it is owed
  /\ retry' = retry \ {<<a, "article">>}                    \* the marker clears
  /\ recv' = IF AdmitsArticle(a, DocOf(a))                  \* sent if it matches,
             THEN [recv EXCEPT ![a]["article"] = sent[a]["article"]]
             ELSE recv                                      \* otherwise not at all
  /\ UNCHANGED <<sent, added, inflight, replayLeft, local>>        \* nothing else moves

\* The retry of a's chunk, decided by its article. A passing chunk is sent
\* together with its article. Without the article here the marker clears;
\* the bug instead fails the retry and keeps the marker, so nothing changes.
RetryChunk(a) ==
  /\ added /\ <<a, "chunk">> \in retry                      \* it is owed
  /\ ~(~Here(a) /\ RequireArticle /\ ~RetryEnds)            \* the bug's failing retry changes nothing
  /\ retry' = retry \ {<<a, "chunk">>}                      \* the marker clears
  /\ LET go == sent[a]["chunk"] # "none" /\ AdmitsChunk(a, DocOf(a)) \* a chunk not here is not sent
         withArticle == go /\ RetrySendsArticle /\ Here(a)  \* and its article with it
     IN recv' = [recv EXCEPT
                   ![a]["chunk"] = IF go THEN sent[a]["chunk"] ELSE @,
                   ![a]["article"] = IF withArticle THEN sent[a]["article"] ELSE @]
  /\ UNCHANGED <<sent, added, inflight, replayLeft, local>>        \* nothing else moves

\* Every step the system can take.
Next ==
  \/ \E a \in Articles, p \in Paths : Create(a, p) \/ Delete(a, p) \* writes on either path
  \/ \E a \in Articles, k \in Kinds : Forward(a, k)                \* forwarded documents
  \/ AddReplicator                                                 \* the replicator is set up
  \/ \E a \in Articles : Replay(a) \/ RetryArticle(a) \/ RetryChunk(a) \* replay and retry
  \/ \E i \in Items : Deliver(i) \/ Lose(i)                        \* pushes arrive or get lost

\* A push on its way is eventually delivered or lost, and the replicator, the
\* replay and every retry that can run eventually do.
Fairness ==
  /\ WF_vars(AddReplicator)
  /\ \A i \in Items : WF_vars(Deliver(i) \/ Lose(i))
  /\ \A a \in Articles : WF_vars(Replay(a)) /\ WF_vars(RetryArticle(a)) /\ WF_vars(RetryChunk(a))

\* Start from Init and take any steps, fairly.
Spec == Init /\ [][Next]_vars /\ Fairness

\* The receiver never holds a document of an article the filter does not match.
INV_NothingUnmatched ==
  \A a \in Articles \ Matches : \A k \in Kinds : recv[a][k] = "none"

\* The receiver holds a chunk only when its article reached it or is still owed to it.
INV_ChunkOnlyWithItsArticle ==
  \A a \in Articles :
    recv[a]["chunk"] # "none" =>
      \/ recv[a]["article"] # "none"
      \/ <<a, "article">> \in inflight \cup retry

\* Every retry eventually ends: none stays failing forever.
LIVE_RetriesEnd == <>[](retry = {})

\* Once a matching article written here reached the receiver, its chunk follows.
LIVE_ChunkFollowsItsArticle ==
  \A a \in Matches :
    (a \in local /\ recv[a]["article"] = "live" /\ sent[a]["chunk"] = "live")
      ~> (recv[a]["chunk"] # "none")
====

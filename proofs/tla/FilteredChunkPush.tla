---- MODULE FilteredChunkPush ----
\* The story: a node (the sender) holds articles whose text is `@chunk`, so
\* each article has chunk documents in a chunk collection of their own. A
\* replicator sends the articles to a second node (the receiver), but only the
\* articles its filter matches. The filter may only name `@immutable` fields,
\* so whether an article matches is fixed when it is created and never
\* changes. Here every article has one chunk, which stands for all of them.
\*
\* A chunk goes to the receiver when its article does. Three paths push a
\* chunk:
\*   - the live push, right after the write that changed it commits;
\*   - the replay, which sends what already exists when the replicator is added;
\*   - the retry, which sends again a push that was lost on the way.
\* A chunk whose article has not arrived yet cannot be decided. The replay
\* then leaves it owed, and the retry waits until the article is here.
\* A write is either a single write or part of an explicit transaction; each
\* hands its live push the article's document. A node can also hold a chunk
\* that arrived from another node before its article did; forwarding it, it
\* has no article document at hand.
\*
\* The rule: a chunk is decided by its article's document and the article's
\* filter, never by the chunk's own document. A chunk document has none of the
\* article's fields, so a filter such as "kind is not secret" would pass on it.
\* Without the article's document a chunk is not sent at all.
\*
\* What must never happen:
\*   - the receiver gets a chunk of an article the filter does not match
\*     (INV_NoChunkOfUnmatchedArticle);
\*   - once nothing is left to send, the receiver misses a chunk, or the
\*     delete of a chunk, of a matching article written on the sender
\*     (INV_SettledHasMatchingChunks), or a chunk the replay left owed once
\*     its article arrived.
\*
\* Code it models:
\*   crates/p2p/src/replicator.rs  ReplicatorInfo::is_filtered_for_collection,
\*     ReplicatorInfo::matches_chunk_source, ReplicatorInfo::matches_filter
\*   crates/p2p/src/sync/coordinator/broadcast.rs  push_jobs (the live push, and
\*     the forward of a merged chunk with no article document)
\*   crates/p2p-adapter/src/lib.rs  with_chunk_collections (chunks of a filtered
\*     article are replicated and recorded with their article)
\*   crates/db/src/merge/broadcast_mutator/mod.rs  BroadcastMutator::publish_derived
\*   crates/db/src/merge/broadcast_mutator/batch.rs  BroadcastBatchMutator::capture_derived
\*   crates/db/src/chunk/sync/write.rs  ChunkWrites::publish (explicit transaction)
\*   crates/db/src/merge/txn_broadcaster.rs  broadcast (explicit transaction)
\*   crates/db/src/merge/push_filter.rs  FilterCheck::decide (replay and retry)
\*   crates/db/src/merge/push_docs.rs  push_existing_docs (an owed chunk keeps a
\*     marker), retry_doc (a chunk without its article keeps its marker)
\* Finite sets are all this model needs.
EXTENDS FiniteSets

CONSTANTS
  Articles,           \* the articles, for example {a1, a2}
  Matches,            \* the articles the filter matches, for example {a1}
  ShipChunks,         \* TRUE: a filtered article's chunks are replicated at all (the fix)
  MarkChunksFiltered, \* TRUE: the chunk collection counts as filtered with its article (the fix)
  DecideByArticle,    \* TRUE: a chunk is decided by its article's document (the fix)
  RequireArticle,     \* TRUE: a chunk with no article document at hand is not sent (the fix)
  TxnCarriesArticle,  \* TRUE: an explicit transaction hands its chunks the article's document (the fix)
  KeepWaiting         \* TRUE: a chunk waiting for its article keeps its marker (the fix)

\* The matching articles are some of the articles.
ASSUME Matches \subseteq Articles
\* Each switch is yes-or-no.
ASSUME ShipChunks \in BOOLEAN
ASSUME MarkChunksFiltered \in BOOLEAN
ASSUME DecideByArticle \in BOOLEAN
ASSUME RequireArticle \in BOOLEAN
ASSUME TxnCarriesArticle \in BOOLEAN
ASSUME KeepWaiting \in BOOLEAN

\* The states a chunk can be in: not there, there, or deleted with its article.
States == {"none", "live", "deleted"}

VARIABLES
  chunk,      \* chunk[a]: the state of article a's chunk on the sender
  article,    \* article[a]: TRUE when the sender holds article a itself
  local,      \* the articles written on the sender, not forwarded from elsewhere
  recv,       \* recv[a]: the state of article a's chunk on the receiver
  added,      \* TRUE once the replicator is added
  inflight,   \* chunks whose push is on its way to the receiver
  retry,      \* chunks whose push was lost and waits for the retry
  replayLeft, \* chunks the replay still has to look at
  owed        \* chunks the replay found without their article, owed once it arrives

\* All the variables, for stuttering steps.
vars == <<chunk, article, local, recv, added, inflight, retry, replayLeft, owed>>

\* The type of every variable.
TypeOK ==
  /\ chunk \in [Articles -> States]     \* each chunk on the sender has a state
  /\ article \in [Articles -> BOOLEAN]  \* each article is held or not
  /\ local \subseteq Articles           \* local writes are of articles
  /\ recv \in [Articles -> States]      \* each chunk on the receiver has a state
  /\ added \in BOOLEAN                  \* the replicator is added or not
  /\ inflight \subseteq Articles        \* pushes on their way are of articles
  /\ retry \subseteq Articles           \* lost pushes are of articles
  /\ replayLeft \subseteq Articles      \* the replay's work is of articles
  /\ owed \subseteq Articles            \* owed chunks are of articles

\* At the start nothing exists and no replicator is set up.
Init ==
  /\ chunk = [a \in Articles |-> "none"]   \* no chunk on the sender
  /\ article = [a \in Articles |-> FALSE]  \* no article on the sender
  /\ local = {}                            \* nothing written yet
  /\ recv = [a \in Articles |-> "none"]    \* no chunk on the receiver
  /\ added = FALSE                         \* no replicator yet
  /\ inflight = {}                         \* nothing on its way
  /\ retry = {}                            \* nothing to retry
  /\ replayLeft = {}                       \* no replay running
  /\ owed = {}                             \* nothing owed

\* Whether the push of article a's chunk goes to the receiver. `doc` says which
\* document is at hand: "article" for the article's, "none" for none at all.
Admits(a, doc) ==
  IF ~ShipChunks THEN FALSE                \* the old code left the chunks out
  ELSE IF ~MarkChunksFiltered THEN TRUE    \* bug: the chunk counts as unfiltered and always goes
  ELSE IF ~DecideByArticle THEN TRUE       \* bug: the chunk's own document passes the filter
  ELSE IF doc = "none" THEN ~RequireArticle \* no article document: fail closed, unless the bug lets it through
  ELSE a \in Matches                       \* the article's document decides, with its filter

\* The document the replay or the retry finds: the article's, when the sender holds it.
Found(a) == IF article[a] THEN "article" ELSE "none"

\* What the replay or the retry does with a's chunk: "send" it, "wait" for its
\* article (the correct code cannot decide without it), or "refuse" it.
Decide(a) ==
  IF Admits(a, Found(a)) THEN "send"           \* admitted: send it
  ELSE IF Found(a) = "none" /\ RequireArticle  \* no article here, and the code fails closed:
  THEN "wait"                                  \* nothing can be decided yet
  ELSE "refuse"                                \* the article does not match

\* A live push of a's chunk is queued when the replicator exists and admits it.
Push(a, doc) ==
  inflight' = IF added /\ Admits(a, doc) THEN inflight \cup {a} ELSE inflight

\* The document a live push gets from a write on path `path`.
WriteDoc(path) == IF path = "txn" /\ ~TxnCarriesArticle THEN "none" ELSE "article"

\* The two ways a write commits: a single write, or an explicit transaction.
Paths == {"single", "txn"}

\* The sender creates article a, with its chunk, in one transaction.
Create(a, path) ==
  /\ chunk[a] = "none" /\ ~article[a]       \* a does not exist anywhere here yet
  /\ chunk' = [chunk EXCEPT ![a] = "live"]   \* the chunk is written
  /\ article' = [article EXCEPT ![a] = TRUE] \* with its article
  /\ local' = local \cup {a}                 \* by this node
  /\ Push(a, WriteDoc(path))                 \* and its live push decides
  /\ UNCHANGED <<recv, added, retry, replayLeft, owed>> \* nothing else moves

\* The sender deletes article a, and its chunk with it, in one transaction.
Delete(a, path) ==
  /\ chunk[a] = "live" /\ article[a]           \* a is live here
  /\ chunk' = [chunk EXCEPT ![a] = "deleted"]  \* the chunk is deleted
  /\ Push(a, WriteDoc(path))                   \* and its live push decides
  /\ UNCHANGED <<article, local, recv, added, retry, replayLeft, owed>> \* the deleted article's data stays

\* A chunk arrives from another node before its article and is forwarded on.
ForwardChunk(a) ==
  /\ chunk[a] = "none" /\ ~article[a]       \* neither is here yet
  /\ chunk' = [chunk EXCEPT ![a] = "live"]   \* the chunk merges here
  /\ Push(a, "none")                         \* its forward has no article document
  /\ UNCHANGED <<article, local, recv, added, retry, replayLeft, owed>> \* nothing else moves

\* The forwarded chunk's article arrives later.
ForwardArticle(a) ==
  /\ chunk[a] # "none" /\ ~article[a]        \* the chunk came first
  /\ article' = [article EXCEPT ![a] = TRUE]  \* now the article is here too
  /\ UNCHANGED <<chunk, local, recv, added, inflight, retry, replayLeft, owed>> \* nothing else moves

\* The replicator is added; the replay will look at every chunk that exists.
AddReplicator ==
  /\ ~added                                           \* not added yet
  /\ added' = TRUE                                    \* now it is
  /\ replayLeft' = {a \in Articles : chunk[a] # "none"} \* every chunk the sender has
  /\ UNCHANGED <<chunk, article, local, recv, inflight, retry, owed>> \* nothing else moves

\* The replay decides one chunk with the document it finds.
Replay(a) ==
  /\ a \in replayLeft                            \* a is still to be looked at
  /\ replayLeft' = replayLeft \ {a}              \* and now it is
  /\ inflight' = IF Decide(a) = "send" THEN inflight \cup {a} ELSE inflight \* sent if admitted
  /\ retry' = IF Decide(a) = "wait" THEN retry \cup {a} ELSE retry \* a waiting chunk gets a marker
  /\ owed' = IF Decide(a) = "wait" THEN owed \cup {a} ELSE owed     \* and is owed
  /\ UNCHANGED <<chunk, article, local, recv, added>> \* nothing else moves

\* A push arrives: the receiver pulls the chunk's current head.
Deliver(a) ==
  /\ a \in inflight                             \* a's push is on its way
  /\ recv' = [recv EXCEPT ![a] = chunk[a]]       \* the receiver now has the sender's state
  /\ inflight' = inflight \ {a}                  \* the push is done
  /\ UNCHANGED <<chunk, article, local, added, retry, replayLeft, owed>> \* nothing else moves

\* A push is lost; its marker stays for the retry.
Lose(a) ==
  /\ a \in inflight                     \* a's push is on its way
  /\ inflight' = inflight \ {a}         \* and gets lost
  /\ retry' = retry \cup {a}            \* the retry owes it
  /\ UNCHANGED <<chunk, article, local, recv, added, replayLeft, owed>> \* nothing else moves

\* The retry decides again with the document it finds, then clears the marker.
\* A chunk still waiting for its article keeps the marker and is not retried yet.
Retry(a) ==
  /\ a \in retry                                  \* a is owed
  /\ Decide(a) = "wait" => ~KeepWaiting          \* with the fix, a waiting chunk is left alone
  /\ retry' = retry \ {a}                         \* the marker is cleared
  /\ recv' = IF Decide(a) = "send"                \* when admitted,
             THEN [recv EXCEPT ![a] = chunk[a]]   \* the receiver gets the current head;
             ELSE recv                            \* otherwise nothing is sent
  /\ UNCHANGED <<chunk, article, local, added, inflight, replayLeft, owed>> \* nothing else moves

\* Every step the system can take.
Next ==
  \/ \E a \in Articles, p \in Paths : Create(a, p) \/ Delete(a, p) \* writes on either path
  \/ \E a \in Articles : ForwardChunk(a) \/ ForwardArticle(a)       \* forwarded data
  \/ AddReplicator                                                 \* the replicator is set up
  \/ \E a \in Articles : Replay(a) \/ Deliver(a) \/ Lose(a) \/ Retry(a) \* the pushes

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* Nothing is left to send: the replicator exists and no push, retry or replay is pending.
Settled == added /\ inflight = {} /\ retry = {} /\ replayLeft = {}

\* The receiver never holds a chunk of an article the filter does not match.
INV_NoChunkOfUnmatchedArticle == \A a \in Articles \ Matches : recv[a] = "none"

\* Once settled, the receiver holds every chunk of every matching article
\* written on the sender, or owed by the replay, in the state the sender holds it.
INV_SettledHasMatchingChunks ==
  Settled => \A a \in Matches \cap (local \cup owed) : recv[a] = chunk[a]
====

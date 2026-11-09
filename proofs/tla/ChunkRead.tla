---- MODULE ChunkRead ----
\* The story: an article has a `@chunk` text, and its chunks are documents of
\* their own. Writers edit the text, delete the article, or revoke the reader's
\* access, at any moment. A reader runs queries over the chunk collection.
\*
\* A chunk has no access rules of its own. A query shows a chunk only when
\* three things hold at the query's snapshot: the article is live, the reader
\* may read it, and the article's current text uses the chunk. The check also
\* hashes the chunk's text again, so a chunk that claims another chunk's hash
\* is not shown. Chunks of one article share one decision about that article,
\* made once per query.
\*
\* A reader may also ask for articles ranked by similarity to a query vector.
\* The vector index holds every stored chunk, old and forged ones too. When the
\* article has no read rules, the route reads the nearest chunks first, a few
\* at a time, and widens until one is honest, of a live article, and listed.
\* That picks the article; its score is then its best listed chunk's, read
\* again from its list. The route checks no grant, so an article with read
\* rules is never routed: like an index that runs out, it is scored from its
\* listed chunks, after its access check.
\*
\* A peer may also ask this node for a chunk's block. A chunk can arrive before
\* its article, and an article this node has never seen counts as public under
\* local access rules. So a block is served only for an article that has
\* arrived, under that article's rules.
\*
\* What must never happen:
\*   - a query shows a chunk of a deleted article, or of one the reader may not
\*     read at the query's snapshot (INV_ShownIsLiveAndReadable);
\*   - a query shows a chunk whose text is not a chunk of the article's text
\*     at the query's snapshot (INV_ShownIsTheText);
\*   - a peer gets a chunk's block while the article hasn't arrived, or while
\*     the peer may not read it (INV_ServedIsReadable);
\*   - a similarity query ranks an article it may not show, or ranks it by a
\*     score other than its best current chunk's (INV_RankedIsBestListed).
\*
\* Code it models:
\*   crates/query/src/plan/chunk_parent.rs  parent_hashes (deleted parent), verified_hash
\*   crates/query/src/plan/chunk_gate.rs    ChunkGate::allows, ChunkGate::decide (one decision per parent per query)
\*   crates/query/src/planner/builder/mod.rs maybe_wrap_with_acp_filter (a new gate per query)
\*   crates/db/src/chunk/access.rs         history_parents, read_owners, merged (an unarrived parent owns nothing)
\*   crates/p2p-adapter/src/read_gate.rs    DbBlockReadGate::may_read, DbBlockClassifier::classify (chunk block serving)
\*   crates/db/src/block/verify.rs          load_authorized_block_signature (the chunk branch)
\*   crates/query/src/planner/builder/mod.rs Planner::plan_with_index_info (rows_rejected_above_the_scan: no route under read rules)
\*   crates/query/src/plan/chunk_similarity/search.rs ChunkSearch::run (nearest first, widening, the checks per chunk)
\*   crates/query/src/plan/chunk_similarity/listed.rs best_listed_score (the exhaustive fallback)
\* Merging text and list across peers is ChunkMerge.tla's job, not this one's.
EXTENDS Naturals, FiniteSets

CONSTANTS
  MaxEdits,      \* how many edits the writers make, at most
  CheckDeleted,  \* TRUE: a deleted article shows no chunks (the guard)
  CheckHash,     \* TRUE: the chunk's text is hashed again (the guard)
  MemoPerQuery,  \* TRUE: each query starts with no remembered decision (the guard)
  CheckMerged,   \* TRUE: a block's owner must be an article that has arrived (the guard)
  CheckGranted,  \* TRUE: the reader must be allowed to read the article (the guard)
  CheckListed,   \* TRUE: a chunk shows only if the article's current list names it (the guard)
  RouteChecksList, \* TRUE: the similarity route ranks only by chunks the current list names (the guard)
  RouteOnlyUnruled \* TRUE: an article with read rules is never routed (the guard)

\* Writers edit at least once.
ASSUME MaxEdits \in Nat \ {0}
\* The four switches are yes-or-no.
ASSUME CheckDeleted \in BOOLEAN /\ CheckHash \in BOOLEAN /\ MemoPerQuery \in BOOLEAN
ASSUME CheckMerged \in BOOLEAN
\* The last three switches are yes-or-no too.
ASSUME CheckGranted \in BOOLEAN /\ CheckListed \in BOOLEAN /\ RouteChecksList \in BOOLEAN
\* And the route's read-rules guard.
ASSUME RouteOnlyUnruled \in BOOLEAN

\* The texts an edit may write; t3 is empty, so it has no chunks.
Texts == {"t1", "t2", "t3"}

\* The hashes of the chunks the splitter cuts each text into.
ChunksOf(t) == CASE t = "t1" -> {"a", "b"} [] t = "t2" -> {"c"} [] t = "t3" -> {}

\* Every chunk hash there is.
Hashes == UNION {ChunksOf(t) : t \in Texts}

\* A stored chunk: the hash it claims, and whether its text really has that hash.
Stored == [hash : Hashes, honest : BOOLEAN]

\* How near each honest chunk is to the query vector: larger is nearer. The
\* chunks of t1, which an edit to t2 leaves behind, rank above t2's chunk.
ScoreOf(h) == CASE h = "a" -> 3 [] h = "b" -> 2 [] h = "c" -> 1

\* A forged chunk's vector is whatever its writer chose, so it ranks first.
Score(c) == IF c.honest THEN ScoreOf(c.hash) ELSE 4

\* The largest score in a set of numbers, 0 for none.
Max(S) == IF S = {} THEN 0 ELSE CHOOSE m \in S : \A n \in S : n <= m

\* The best score among the chunks with hashes H.
Best(H) == Max({ScoreOf(h) : h \in H})

\* What a query sees: the article and the chunks, frozen at one moment.
Views == [text : Texts, merged : BOOLEAN, deleted : BOOLEAN, granted : BOOLEAN,
          stored : SUBSET Stored]

\* What a query remembers about the article: nothing yet, or a decision.
NoDecision == [ok |-> FALSE, hashes |-> {}, made |-> FALSE]

VARIABLES
  text,     \* the article's current text
  merged,   \* whether the article has arrived on this node
  deleted,  \* whether the article is deleted
  granted,  \* whether the reader may read the article
  stored,   \* the chunks in the store
  edits,    \* how many edits the writers have made
  phase,    \* the reader: "idle", "reading" (snapshot taken) or "done"
  snap,     \* the snapshot the current query reads
  memo,     \* the decision about the article the reader holds
  result,   \* the chunks the last query showed
  leaked,   \* whether a peer was ever served a block it may not have
  kind,     \* what the current query asks: "chunks", or "similar" for a ranking
  width,    \* how many of the nearest chunks the route has read
  ranked,   \* the article's score in the last ranking, 0 when it was not ranked
  ruled     \* whether the article has read rules; it never changes

\* All the variables, for stuttering steps.
vars == <<text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked,
          kind, width, ranked, ruled>>

\* What a ranking step leaves alone.
rankVars == <<text, merged, deleted, granted, stored, edits, snap, memo, result, leaked, kind, ruled>>

\* The chunks an honest write of text t stores.
HonestChunks(t) == {[hash |-> h, honest |-> TRUE] : h \in ChunksOf(t)}

\* The store's state right now, as a query would see it.
Now == [text |-> text, merged |-> merged, deleted |-> deleted, granted |-> granted,
        stored |-> stored]

\* The decision about the article from view v: may its chunks show, and which.
Decide(v) ==
  [ok |-> (v.merged /\ (CheckGranted => v.granted) /\ (CheckDeleted => ~v.deleted)),  \* arrived, readable (with the guard), live
   hashes |-> ChunksOf(v.text),                          \* the chunks its text uses
   made |-> TRUE]                                        \* a decision exists now

\* Whether a stored chunk passes the hash check.
HashOK(c) == CheckHash => c.honest

\* Whether decision d lists chunk c. Without the guard every stored chunk counts as listed.
ListedOK(c, d) == CheckListed => c.hash \in d.hashes

\* Whether serving a chunk's block is allowed: the owner is the article, which
\* with the guard must have arrived. An article that hasn't arrived has no
\* access rules here, so it counts as public.
ServeAllowed ==
  /\ CheckMerged => merged                           \* the guard: the owner is known
  /\ merged => granted                               \* a known article's rules apply

\* The nearest `w` chunks in the snapshot's index, nearest first. Chunks that
\* tie are read together.
Prefix(w) == {c \in snap.stored :
                Cardinality({e \in snap.stored : Score(e) > Score(c)}) < w}

\* Whether the ranking may use the route: with the guard, only without read rules.
Routes == RouteOnlyUnruled => ~ruled

\* The chunks in S the route may rank the article by: honest, of an article that
\* has arrived and is live, and with the guard named by its current list. Like
\* the code, the route checks no grant.
Accepted(S) ==
  LET live == snap.merged /\ (CheckDeleted => ~snap.deleted)  \* the route's parent check
  IN {c \in S : HashOK(c) /\ live /\ (RouteChecksList => c.hash \in ChunksOf(snap.text))}

\* The exhaustive answer: the best of the article's listed, honest chunks, or 0
\* when the article may not show.
Exhaustive ==
  LET d == Decide(snap)                                   \* the same decision
  IN IF d.ok
     THEN Max({Score(c) : c \in {e \in snap.stored : HashOK(e) /\ e.hash \in d.hashes}})
     ELSE 0

\* The type of every variable.
TypeOK ==
  /\ text \in Texts                                   \* a text
  /\ merged \in BOOLEAN                               \* arrived or not
  /\ deleted \in BOOLEAN /\ granted \in BOOLEAN        \* two flags
  /\ stored \subseteq Stored                           \* stored chunks
  /\ edits \in 0..MaxEdits                             \* edits made
  /\ phase \in {"idle", "reading", "done"}             \* reader phase
  /\ snap \in Views                                    \* a snapshot
  /\ memo \in [ok : BOOLEAN, hashes : SUBSET Hashes, made : BOOLEAN]  \* a decision or none
  /\ result \subseteq Stored                           \* shown chunks
  /\ leaked \in BOOLEAN                               \* a bad serve happened
  /\ kind \in {"chunks", "similar"}                   \* a query kind
  /\ width \in Nat                                    \* chunks read so far
  /\ ranked \in 0..4                                  \* a score, or 0
  /\ ruled \in BOOLEAN                                \* read rules or none

\* At the start the chunks of t1 are here but the article hasn't arrived yet,
\* and the reader may read the article.
Init ==
  /\ text = "t1"                           \* first text
  /\ merged = FALSE                        \* the article is still on its way
  /\ deleted = FALSE                       \* live
  /\ granted = TRUE                        \* readable
  /\ stored = HonestChunks("t1")           \* t1's chunks are stored
  /\ edits = 0                             \* no edits yet
  /\ phase = "idle"                        \* no query running
  /\ snap = Now                            \* any view will do before a query
  /\ memo = NoDecision                     \* nothing remembered
  /\ result = {}                           \* nothing shown
  /\ leaked = FALSE                        \* nothing served yet
  /\ kind = "chunks"                       \* any kind before a query
  /\ width = 1                             \* the route starts with one chunk
  /\ ranked = 0                            \* nothing ranked
  /\ ruled \in BOOLEAN                     \* the article has read rules, or not

\* A writer edits the live article to text t, storing t's chunks with it.
Edit(t) ==
  /\ ~deleted /\ edits < MaxEdits                   \* live, edits left
  /\ text' = t                                      \* the new text
  /\ stored' = stored \cup HonestChunks(t)          \* and its chunks
  /\ edits' = edits + 1                             \* one more edit
  /\ UNCHANGED <<merged, deleted, granted, phase, snap, memo, result, leaked, kind, width, ranked, ruled>>

\* A writer deletes the article. Its chunks stay stored.
Delete ==
  /\ ~deleted                                       \* not yet deleted
  /\ deleted' = TRUE                                \* now deleted
  /\ UNCHANGED <<merged, text, granted, stored, edits, phase, snap, memo, result, leaked, kind, width, ranked, ruled>>

\* The owner revokes the reader's access.
Revoke ==
  /\ ruled /\ granted                               \* rules to revoke under, still granted
  /\ granted' = FALSE                               \* now revoked
  /\ UNCHANGED <<merged, text, deleted, stored, edits, phase, snap, memo, result, leaked, kind, width, ranked, ruled>>

\* A chunk that claims hash h but holds other text reaches the store by some
\* route that skipped the write guard.
Plant(h) ==
  /\ stored' = stored \cup {[hash |-> h, honest |-> FALSE]}  \* the forged chunk
  /\ UNCHANGED <<merged, text, deleted, granted, edits, phase, snap, memo, result, leaked, kind, width, ranked, ruled>>

\* A query of kind k starts: it takes its snapshot, and with the guard forgets
\* old decisions.
Start(k) ==
  /\ phase \in {"idle", "done"}                     \* no query running
  /\ phase' = "reading"                             \* one is now
  /\ kind' = k                                      \* chunks or a ranking
  /\ snap' = Now                                    \* reading this moment
  /\ memo' = IF MemoPerQuery THEN NoDecision ELSE memo  \* the guard: a fresh gate
  /\ result' = {}                                   \* nothing shown yet
  /\ width' = 1                                     \* the route reads one chunk first
  /\ ranked' = 0                                    \* nothing ranked yet
  /\ UNCHANGED <<text, merged, deleted, granted, stored, edits, leaked, ruled>>

\* The query shows the chunks the decision allows, deciding first if needed.
Answer ==
  /\ phase = "reading" /\ kind = "chunks"           \* a chunk query is running
  /\ LET d == IF memo.made THEN memo ELSE Decide(snap)  \* reuse or decide
     IN /\ memo' = d                                \* remember the decision
        /\ result' = {c \in snap.stored :           \* the stored chunks that
             HashOK(c) /\ d.ok /\ ListedOK(c, d)}  \* pass every check
  /\ phase' = "done"                                \* the query has answered
  /\ UNCHANGED <<text, merged, deleted, granted, stored, edits, snap, leaked, kind, width, ranked, ruled>>

\* The route finds no chunk it may rank by among the nearest `width`, and the
\* index holds more, so it reads twice as many.
Widen ==
  /\ phase = "reading" /\ kind = "similar"          \* a ranking is running
  /\ Routes                                         \* that may use the route
  /\ Accepted(Prefix(width)) = {}                   \* nothing to rank by yet
  /\ width < Cardinality(snap.stored)                \* the index has more
  /\ width' = width * 2                              \* read twice as many
  /\ UNCHANGED <<rankVars, phase, ranked>>

\* The route found a chunk it may rank by: the best one read is the score.
RankRouted ==
  /\ phase = "reading" /\ kind = "similar"          \* a ranking is running
  /\ Routes                                         \* that may use the route
  /\ Accepted(Prefix(width)) # {}                   \* something to rank by
  /\ LET found == Max({Score(c) : c \in Accepted(Prefix(width))})  \* the best chunk read
         exact == Max({Score(c) : c \in {e \in snap.stored :           \* the best listed chunk,
                        HashOK(e) /\ e.hash \in ChunksOf(snap.text)}})  \* read again
     IN ranked' = IF exact > 0 THEN exact ELSE found      \* the code keeps what it read if none
  /\ phase' = "done"                                \* the ranking has answered
  /\ UNCHANGED <<rankVars, width>>

\* No route, or the index ran out first: the article's listed chunks are scored
\* one by one, after its access check.
RankExhaustive ==
  /\ phase = "reading" /\ kind = "similar"          \* a ranking is running
  /\ \/ ~Routes                                     \* it may not route,
     \/ /\ Accepted(Prefix(width)) = {}              \* or nothing to rank by
        /\ width >= Cardinality(snap.stored)          \* and nothing more to read
  /\ ranked' = Exhaustive                           \* the exhaustive answer
  /\ phase' = "done"                                \* the ranking has answered
  /\ UNCHANGED <<rankVars, width>>

\* The article arrives on this node, with its access rules.
Arrive ==
  /\ ~merged                                        \* not here yet
  /\ merged' = TRUE                                 \* here now
  /\ UNCHANGED <<text, deleted, granted, stored, edits, phase, snap, memo, result, leaked, kind, width, ranked, ruled>>

\* A peer that may read only what the reader may asks for a stored chunk's block.
\* It is served when allowed; a serve the peer may not have is a leak.
Serve ==
  /\ stored # {}                                    \* there is a block to ask for
  /\ ServeAllowed                                   \* this node hands it over
  /\ leaked' = (leaked \/ ~(merged /\ granted))     \* bad if the article is unknown or unreadable
  /\ UNCHANGED <<text, merged, deleted, granted, stored, edits, phase, snap, memo, result, kind, width, ranked, ruled>>

\* Every step anyone can take.
Next ==
  \/ \E t \in Texts : Edit(t)    \* an edit,
  \/ Delete                      \* a delete,
  \/ Revoke                      \* a revoke,
  \/ \E h \in Hashes : Plant(h)  \* a forged chunk,
  \/ Arrive                      \* the article arrives,
  \/ Serve                       \* a peer is served a block,
  \/ \E k \in {"chunks", "similar"} : Start(k)  \* a query starts,
  \/ Answer                      \* a chunk query answers,
  \/ Widen                       \* a ranking reads more chunks,
  \/ RankRouted                  \* a ranking answers from the index,
  \/ RankExhaustive              \* or from every listed chunk

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* A shown chunk means the article was live and readable at the query's snapshot.
INV_ShownIsLiveAndReadable ==
  (phase = "done" /\ result # {}) => (snap.granted /\ ~snap.deleted)

\* A shown chunk is an honest chunk of the article's text at the query's snapshot.
INV_ShownIsTheText ==
  phase = "done" => \A c \in result : c.honest /\ c.hash \in ChunksOf(snap.text)
\* A peer is served a chunk's block only for an article that has arrived and that it may read.
INV_ServedIsReadable == ~leaked

\* A ranking scores a live, readable article by its best current chunk, and
\* leaves out an article it may not show. It reads the snapshot itself, so a
\* RED that turns a guard off can't weaken it.
INV_RankedIsBestListed ==
  (phase = "done" /\ kind = "similar") =>
    ranked = IF snap.merged /\ snap.granted /\ ~snap.deleted
             THEN Best(ChunksOf(snap.text)) ELSE 0
====

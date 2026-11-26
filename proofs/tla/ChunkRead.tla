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
\* The article stores no list of its chunks, so the decision cuts the current
\* text itself to learn which chunks it uses.
\*
\* An app read validator may also govern the chunk collection itself. It can
\* refuse a reader a chunk the article would let them see, so a chunk shows
\* only when that check passes too.
\*
\* A reader may also ask for articles ranked by similarity to a query vector.
\* The vector index holds every stored chunk, old and forged ones too. When the
\* article has no read rules, the route reads the nearest chunks first, a few
\* at a time, and widens until one is honest, of a live article, and used by
\* its current text. That picks the article; its score is then its best
\* current chunk's, read again, every copy of each hash counted. The route
\* checks no grant, so an article with read rules, or a chunk collection with
\* an app check of its own, is never routed: like an index that runs out, it
\* is scored from its current chunks, after its access check. A chunk its own
\* app check refuses never adds to a score.
\*
\* A peer may also ask this node for a chunk's block. A chunk can arrive before
\* its article, and an article this node has never seen counts as public under
\* local access rules. So a block is served only for an article that has
\* arrived, under that article's rules.
\*
\* To find those rules, the node must also know the chunk's collection and the
\* article collection that chunks it. A node that can't find them would see a
\* collection with no rules, which counts as public. So it serves nothing
\* until it finds them. Releasing a chunk's key and pushing a chunk's event
\* follow the same rule as serving its block.
\*
\* A peer may also ask for a key while replaying for the reader, under a
\* delegation bound to one collection: the article's, or its chunk
\* collection's. A key belongs to a block of one of the two. A delegation on
\* the article covers its chunks too, but one on the chunk collection covers
\* only the chunks. The node knows which collection a block is in, so it
\* checks the delegation against that collection, not just against the article.
\*
\* What must never happen:
\*   - a query shows a chunk of a deleted article, or of one the reader may not
\*     read at the query's snapshot (INV_ShownIsLiveAndReadable);
\*   - a query shows a chunk whose text is not a chunk of the article's text
\*     at the query's snapshot (INV_ShownIsTheText);
\*   - a query shows a chunk the chunk collection's own app check refuses at
\*     the query's snapshot (INV_ShownPassesOwnApp);
\*   - a peer gets a chunk's block while the article hasn't arrived, or while
\*     the peer may not read it (INV_ServedIsReadable);
\*   - a peer replaying under a delegation on the chunk collection gets the key
\*     of one of the article's own blocks (INV_DelegatedKeyInScope);
\*   - a similarity query ranks an article it may not show, or ranks it by a
\*     score other than its best current chunk's, or by a chunk the chunk
\*     collection's own app check refuses (INV_RankedIsBestListed).
\*
\* Code it models:
\*   crates/engine/query/src/plan/chunk_parent.rs  parent_hashes (deleted parent, the text cut again), ChunkFields::verified_hash
\*   crates/engine/query/src/plan/chunk_gate.rs    ChunkGate::allows, ChunkGate::decide (one decision per parent per query)
\*   crates/engine/query/src/plan/permission_filter.rs PermissionFilterNode::permission_check (the chunk's fields
\*                                          come from its scan row, read at the query's snapshot)
\*   crates/engine/query/src/planner/builder/mod.rs maybe_wrap_with_acp_filter (a new gate per query)
\*   crates/engine/query/src/runner/chunk_access.rs chunk_readable, ChunkReads (commits, arrivals, updates and
\*                                          version reads; one decision per parent per request, then
\*                                          the chunk collection's own app check per chunk)
\*   crates/engine/query/src/planner/builder/mod.rs maybe_wrap_with_acp_filter (the chunk collection's own app check)
\*   crates/database/db/src/chunk/access.rs         history_parents, read_owners, merged (an unarrived parent owns nothing)
\*   crates/database/db/src/chunk/access.rs         parent_collection, read_owners (an unknown collection owns nothing)
\*   crates/database/db/src/chunk/access.rs         read_owners (each owner keeps its block's collection)
\*   crates/database/kms/src/defra_kms.rs           DefraKms::local_release_decision (a delegated key release)
\*   crates/database/kms/src/nac_dac_policy.rs      Delegation::covers (the delegation's collection against the block's)
\*   crates/surfaces/p2p-adapter/src/read_gate.rs    DbBlockReadGate::may_read, DbBlockClassifier::classify (chunk block serving)
\*   crates/database/db/src/block/verify.rs          load_authorized_block_signature (the chunk branch)
\*   crates/surfaces/http/src/handlers/events.rs     can_observe_document_event (an unknown collection's event)
\*   crates/engine/query/src/planner/builder/mod.rs Planner::plan_with_index_info (rows_rejected_above_the_scan: no route under read rules)
\*   crates/engine/query/src/plan/chunk_similarity/search.rs ChunkSearch::run (nearest first, widening, the checks per chunk)
\*   crates/engine/query/src/plan/chunk_similarity/listed.rs best_listed_score (the exhaustive fallback)
\*   crates/engine/query/src/plan/chunk_similarity/listed.rs best_score_of, Held (every copy of a hash counts)
\*   crates/engine/query/src/plan/chunk_similarity/target.rs ChunkTarget::readable (the chunk's own app check in a ranking)
\*   crates/engine/query/src/planner/chunk_similarity.rs Planner::chunks_gate_reads (no route under the chunks' own app check)
\* Merging the text across peers is ChunkMerge.tla's job, not this one's.
EXTENDS Naturals, FiniteSets

CONSTANTS
  MaxEdits,      \* how many edits the writers make, at most
  CheckDeleted,  \* TRUE: a deleted article shows no chunks (the guard)
  CheckHash,     \* TRUE: the chunk's text is hashed again (the guard)
  MemoPerQuery,  \* TRUE: each query starts with no remembered decision (the guard)
  CheckMerged,   \* TRUE: a block's owner must be an article that has arrived (the guard)
  CheckGranted,  \* TRUE: the reader must be allowed to read the article (the guard)
  CheckCut,      \* TRUE: a chunk shows only if a cut of the article's current text has it (the guard)
  CheckResolved, \* TRUE: nothing is served while the chunk's collections are unknown here (the guard)
  CheckOwnApp,   \* TRUE: the chunk collection's own app check runs too (the guard)
  CheckDelegationScope, \* TRUE: a delegation is checked against the collection of the block (the guard)
  RouteChecksList, \* TRUE: the similarity route ranks only by chunks the current text uses (the guard)
  RouteOnlyUnruled, \* TRUE: an article with read rules is never routed (the guard)
  HeldKeepsBest  \* TRUE: a rescore keeps every copy of a hash, not the last one the route read (the guard)

\* Writers edit at least once.
ASSUME MaxEdits \in Nat \ {0}
\* The four switches are yes-or-no.
ASSUME CheckDeleted \in BOOLEAN /\ CheckHash \in BOOLEAN /\ MemoPerQuery \in BOOLEAN
ASSUME CheckMerged \in BOOLEAN
\* The last two switches are yes-or-no too.
ASSUME CheckGranted \in BOOLEAN /\ CheckCut \in BOOLEAN
\* And the resolve switch.
ASSUME CheckResolved \in BOOLEAN
\* And the own app check switch.
ASSUME CheckOwnApp \in BOOLEAN
\* And the delegation scope switch.
ASSUME CheckDelegationScope \in BOOLEAN
\* And the similarity route's two switches.
ASSUME RouteChecksList \in BOOLEAN /\ RouteOnlyUnruled \in BOOLEAN
\* And the rescore's switch.
ASSUME HeldKeepsBest \in BOOLEAN

\* The collections a block can be in, and a delegation bound to: the article's or its chunk collection.
Collections == {"parent", "chunk"}

\* Whether a delegation on collection d covers a block in collection b: one on
\* the article covers every block, one on the chunk collection only its own.
DelegationCovers(d, b) == d = "parent" \/ d = b

\* The texts an edit may write; t3 is empty, so it has no chunks.
Texts == {"t1", "t2", "t3"}

\* The hashes of the chunks the splitter cuts each text into.
ChunksOf(t) == CASE t = "t1" -> {"a", "b"} [] t = "t2" -> {"c"} [] t = "t3" -> {}

\* Every chunk hash there is.
Hashes == UNION {ChunksOf(t) : t \in Texts}

\* A stored chunk: the hash it claims, whether its text really has that hash,
\* and which copy it is. Peers that sign or encrypt keep a chunk each, so one
\* hash can have two honest copies, each with its writer's vector.
Stored == [hash : Hashes, honest : BOOLEAN, copy : {1, 2}]

\* How near each hash's first honest copy is to the query vector: larger is
\* nearer. The chunks of t1, which an edit to t2 leaves behind, rank above t2's.
ScoreOf(h) == CASE h = "a" -> 6 [] h = "b" -> 4 [] h = "c" -> 2

\* A second copy's vector came from another embedding, a little farther away.
\* A forged chunk's vector is whatever its writer chose, so it ranks first.
Score(c) == IF c.honest THEN ScoreOf(c.hash) - (c.copy - 1) ELSE 7

\* The largest score in a set of numbers, 0 for none.
Max(S) == IF S = {} THEN 0 ELSE CHOOSE m \in S : \A n \in S : n <= m

\* The best score among view v's honest chunks that its text uses, every copy counted.
BestStored(v) == Max({Score(c) : c \in {e \in v.stored : e.honest /\ e.hash \in ChunksOf(v.text)}})

\* What a query sees: the article and the chunks, frozen at one moment.
Views == [text : Texts, merged : BOOLEAN, deleted : BOOLEAN, granted : BOOLEAN,
          stored : SUBSET Stored, ownApp : BOOLEAN]

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
  resolved, \* whether this node knows the chunk collection and the article collection that chunks it
  ownApp,   \* whether the chunk collection's own app check lets the reader read its chunks
  scopeLeaked, \* whether a delegated peer ever got a key its delegation doesn't cover
  kind,     \* what the current query asks: "chunks", or "similar" for a ranking
  width,    \* how many of the nearest chunks the route has read
  ranked,   \* the article's score in the last ranking, 0 when it was not ranked
  ruled     \* whether the article or its chunk collection has read rules; it never changes

\* All the variables, for stuttering steps.
vars == <<text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked, resolved, ownApp, scopeLeaked,
          kind, width, ranked, ruled>>

\* What a ranking step leaves alone.
rankVars == <<text, merged, deleted, granted, stored, edits, snap, memo, result, leaked, resolved, ownApp, scopeLeaked,
              kind, ruled>>

\* The chunks an honest write of text t stores.
HonestChunks(t) == {[hash |-> h, honest |-> TRUE, copy |-> 1] : h \in ChunksOf(t)}

\* The store's state right now, as a query would see it.
Now == [text |-> text, merged |-> merged, deleted |-> deleted, granted |-> granted,
        stored |-> stored, ownApp |-> ownApp]


\* The decision about the article from view v: may its chunks show, and which.
Decide(v) ==
  [ok |-> (v.merged /\ (CheckGranted => v.granted) /\ (CheckDeleted => ~v.deleted)),  \* arrived, readable (with the guard), live
   hashes |-> ChunksOf(v.text),                          \* the text cut again
   made |-> TRUE]                                        \* a decision exists now

\* Whether a stored chunk passes the hash check.
HashOK(c) == CheckHash => c.honest

\* Whether decision d's cut of the text has chunk c. Without the guard every stored chunk counts.
CutOK(c, d) == CheckCut => c.hash \in d.hashes

\* Whether serving a chunk's block is allowed: the owner is the article, which
\* with the guard must have arrived. An article that hasn't arrived has no
\* access rules here, so it counts as public. The same holds when the node
\* can't find the collections, since then it finds no rules at all.
ServeAllowed ==
  /\ CheckResolved => resolved                       \* the guard: the collections are known
  /\ CheckMerged => merged                           \* the guard: the owner is known
  /\ (resolved /\ merged) => granted                 \* a known article's rules apply

\* The nearest `w` chunks in the snapshot's index, nearest first. Chunks that
\* tie are read together.
Prefix(w) == {c \in snap.stored :
                Cardinality({e \in snap.stored : Score(e) > Score(c)}) < w}

\* Whether the ranking may use the route: with the guard, only without read rules.
Routes == RouteOnlyUnruled => ~ruled

\* Whether the chunk collection's own app check, with the guard, lets the
\* reader see the snapshot's chunks.
OwnAppOK == CheckOwnApp => snap.ownApp

\* The chunks in S the route may rank the article by: honest, of an article that
\* has arrived and is live, passing their own app check, and with the guard
\* used by its current text. Like the code, the route checks no grant.
Accepted(S) ==
  LET live == snap.merged /\ (CheckDeleted => ~snap.deleted)  \* the route's parent check
  IN {c \in S : HashOK(c) /\ live /\ OwnAppOK                \* honest, live, own app check
                /\ (RouteChecksList => c.hash \in ChunksOf(snap.text))}  \* with the guard, listed

\* The exhaustive answer: the best of the article's current, honest chunks, or 0
\* when the article may not show or its chunks' own app check refuses them.
Exhaustive ==
  LET d == Decide(snap)                                   \* the same decision
  IN IF d.ok /\ OwnAppOK                                  \* the article and its chunks may show
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
  /\ resolved \in BOOLEAN                             \* collections known or not
  /\ ownApp \in BOOLEAN                               \* own app check passes or not
  /\ scopeLeaked \in BOOLEAN                          \* an out of scope key went out
  /\ kind \in {"chunks", "similar"}                   \* a query kind
  /\ width \in Nat                                    \* chunks read so far
  /\ ranked \in 0..7                                  \* a score, or 0
  /\ ruled \in BOOLEAN                                \* read rules or none

\* At the start the chunks of t1 are here but the article hasn't arrived yet,
\* and the reader may read the article.
Init ==
  /\ text = "t1"                           \* first text
  /\ merged = FALSE                        \* the article is still on its way
  /\ deleted = FALSE                       \* live
  /\ granted = TRUE                        \* readable
  /\ ownApp = TRUE                         \* the own app check passes
  /\ stored = HonestChunks("t1")           \* t1's chunks are stored
  /\ edits = 0                             \* no edits yet
  /\ phase = "idle"                        \* no query running
  /\ snap = Now                            \* any view will do before a query
  /\ memo = NoDecision                     \* nothing remembered
  /\ result = {}                           \* nothing shown
  /\ leaked = FALSE                        \* nothing served yet
  /\ resolved = FALSE                      \* the collections haven't arrived either
  /\ scopeLeaked = FALSE                   \* no delegated key went out yet
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
  /\ UNCHANGED <<scopeLeaked, merged, deleted, granted, phase, snap, memo, result, leaked, resolved, ownApp, kind, width, ranked, ruled>>

\* A writer deletes the article. Its chunks stay stored.
Delete ==
  /\ ~deleted                                       \* not yet deleted
  /\ deleted' = TRUE                                \* now deleted
  /\ UNCHANGED <<scopeLeaked, merged, text, granted, stored, edits, phase, snap, memo, result, leaked, resolved, ownApp, kind, width, ranked, ruled>>

\* The owner revokes the reader's access.
Revoke ==
  /\ ruled /\ granted                               \* rules to revoke under, still granted
  /\ granted' = FALSE                               \* now revoked
  /\ UNCHANGED <<scopeLeaked, merged, text, deleted, stored, edits, phase, snap, memo, result, leaked, resolved, ownApp, kind, width, ranked, ruled>>

\* A chunk that claims hash h but holds other text reaches the store by some
\* route that skipped the write guard.
Plant(h) ==
  /\ \A c \in stored : c.honest                     \* one forged chunk is enough to show the check
  /\ stored' = stored \cup {[hash |-> h, honest |-> FALSE, copy |-> 1]}  \* the forged chunk
  /\ UNCHANGED <<scopeLeaked, merged, text, deleted, granted, edits, phase, snap, memo, result, leaked, resolved, ownApp, kind, width, ranked, ruled>>

\* A peer that embeds differently writes the same text, so hash h of the
\* current text gets a second honest copy with its own vector.
Reembed(h) ==
  /\ h \in ChunksOf(text)                                         \* a chunk the text uses
  /\ [hash |-> h, honest |-> TRUE, copy |-> 1] \in stored          \* whose first copy is here
  /\ \A c \in stored : c.copy = 1                                  \* one second copy is enough
  /\ stored' = stored \cup {[hash |-> h, honest |-> TRUE, copy |-> 2]}  \* and now a second one
  /\ UNCHANGED <<scopeLeaked, merged, text, deleted, granted, edits, phase, snap, memo, result, leaked, resolved, ownApp, kind, width, ranked, ruled>>

\* The chunk collection's own app check starts refusing the reader. Such a
\* check is a read rule, so it exists only where `ruled` holds.
AppRefuse ==
  /\ ruled /\ ownApp                               \* a rule that still passes
  /\ ownApp' = FALSE                               \* refusing now
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked, resolved, kind, width, ranked, ruled>>

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
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, leaked, resolved, ownApp, ruled>>

\* The query shows the chunks the decision allows, deciding first if needed.
Answer ==
  /\ phase = "reading" /\ kind = "chunks"           \* a chunk query is running
  /\ LET d == IF memo.made THEN memo ELSE Decide(snap)  \* reuse or decide
     IN /\ memo' = d                                \* remember the decision
        /\ result' = {c \in snap.stored :           \* the stored chunks that
             HashOK(c) /\ d.ok /\ CutOK(c, d)      \* pass every check on the article
             /\ (CheckOwnApp => snap.ownApp)}       \* and, with the guard, the own app check
  /\ phase' = "done"                                \* the query has answered
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, snap, leaked, resolved, ownApp, kind, width, ranked, ruled>>

\* The route finds no chunk it may rank by among the nearest `width`, and the
\* index holds more, so it reads twice as many.
Widen ==
  /\ phase = "reading" /\ kind = "similar"          \* a ranking is running
  /\ Routes                                         \* that may use the route
  /\ Accepted(Prefix(width)) = {}                   \* nothing to rank by yet
  /\ width < Cardinality(snap.stored)                \* the index has more
  /\ width' = width * 2                              \* read twice as many
  /\ UNCHANGED <<rankVars, phase, ranked>>

\* The route found a chunk it may rank by: the article is picked, and its
\* score is its best current chunk's, read again. With the guard the rescore
\* counts every copy of each hash. Without it, the route holds one score per
\* hash, the last copy it read, and the rescore skips reading a held hash.
RankRouted ==
  /\ phase = "reading" /\ kind = "similar"          \* a ranking is running
  /\ Routes                                         \* that may use the route
  /\ Accepted(Prefix(width)) # {}                   \* something to rank by
  /\ LET read == Accepted(Prefix(width))                           \* the chunks it may rank by
         found == Max({Score(c) : c \in read})                     \* the best chunk read
         current == {e \in snap.stored :                           \* the article's current chunks
                       HashOK(e) /\ OwnAppOK /\ e.hash \in ChunksOf(snap.text)}
         held == {c.hash : c \in read} \cap ChunksOf(snap.text)    \* the listed hashes it read
     IN \E last \in [held -> read] :                               \* the copy read last, per hash
          /\ \A h \in held : last[h].hash = h                       \* is a copy of that hash
          /\ LET exact == IF HeldKeepsBest
                          THEN Max({Score(c) : c \in current})     \* every copy, read again
                          ELSE Max({Score(last[h]) : h \in held}   \* the last held copy,
                                   \cup {Score(e) : e \in {c \in current : c.hash \notin held}})  \* and the rest
             IN ranked' = IF exact > 0 THEN exact ELSE found  \* the code keeps what it read if none
  /\ phase' = "done"                                \* the ranking has answered
  /\ UNCHANGED <<rankVars, width>>

\* No route, or the index ran out first: the article's current chunks are
\* scored one by one, after its access check.
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
  /\ UNCHANGED <<scopeLeaked, text, deleted, granted, stored, edits, phase, snap, memo, result, leaked, resolved, ownApp, kind, width, ranked, ruled>>

\* The chunk collection and the article collection that chunks it become known here.
Resolve ==
  /\ ~resolved                                      \* not known yet
  /\ resolved' = TRUE                               \* known now
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked, ownApp, kind, width, ranked, ruled>>

\* A peer that may read only what the reader may asks for a stored chunk's block.
\* It is served when allowed; a serve the peer may not have is a leak.
Serve ==
  /\ stored # {}                                    \* there is a block to ask for
  /\ ServeAllowed                                   \* this node hands it over
  /\ leaked' = (leaked \/ ~(merged /\ granted))     \* bad if the article is unknown or unreadable
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, phase, snap, memo, result, resolved, ownApp, kind, width, ranked, ruled>>

\* A peer replaying for the reader under a delegation on collection d asks for
\* the key of a block in collection b. The key follows the article's rules, as
\* serving a block does. Without the guard, any of the article's collections
\* counts as covering every block of the article.
DelegatedKey(d, b) ==
  /\ ServeAllowed                                    \* the article's rules let the key go
  /\ CheckDelegationScope => DelegationCovers(d, b)  \* the guard: the delegation covers the block
  /\ scopeLeaked' = (scopeLeaked \/ ~DelegationCovers(d, b))  \* bad if it doesn't
  /\ UNCHANGED <<text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked, resolved, ownApp, kind, width, ranked, ruled>>

\* Every step anyone can take.
Next ==
  \/ \E t \in Texts : Edit(t)    \* an edit,
  \/ Delete                      \* a delete,
  \/ Revoke                      \* a revoke,
  \/ \E h \in Hashes : Plant(h)  \* a forged chunk,
  \/ \E h \in Hashes : Reembed(h)  \* a second honest copy,
  \/ Arrive                      \* the article arrives,
  \/ Resolve                     \* the collections become known,
  \/ AppRefuse                   \* the own app check refuses,
  \/ Serve                       \* a peer is served a block,
  \/ \E d, b \in Collections : DelegatedKey(d, b)  \* a delegated peer asks for a key,
  \/ \E k \in {"chunks", "similar"} : Start(k)  \* a query starts,
  \/ Answer                      \* a chunk query answers,
  \/ Widen                       \* a ranking reads more chunks,
  \/ RankRouted                  \* a ranking answers from the index,
  \/ RankExhaustive              \* or from every current chunk

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* A shown chunk means the article was live and readable at the query's snapshot.
INV_ShownIsLiveAndReadable ==
  (phase = "done" /\ result # {}) => (snap.granted /\ ~snap.deleted)

\* A shown chunk is an honest chunk of the article's text at the query's snapshot.
INV_ShownIsTheText ==
  phase = "done" => \A c \in result : c.honest /\ c.hash \in ChunksOf(snap.text)
\* A shown chunk passed the chunk collection's own app check at the query's snapshot.
INV_ShownPassesOwnApp == (phase = "done" /\ result # {}) => snap.ownApp

\* A peer is served a chunk's block only for an article that has arrived and that it may read.
INV_ServedIsReadable == ~leaked

\* A peer replaying under a delegation gets only keys of blocks its delegation covers.
INV_DelegatedKeyInScope == ~scopeLeaked

\* A ranking scores a live, readable article by its best current chunk, and
\* leaves out an article it may not show or whose chunks their own app check
\* refuses. It reads the snapshot itself, so a RED that turns a guard off
\* can't weaken it.
INV_RankedIsBestListed ==
  (phase = "done" /\ kind = "similar") =>
    ranked = IF snap.merged /\ snap.granted /\ ~snap.deleted /\ snap.ownApp
             THEN BestStored(snap) ELSE 0
====

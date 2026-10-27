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
\*     the peer may not read it (INV_ServedIsReadable).
\*
\* Code it models:
\*   crates/query/src/plan/chunk_parent.rs  parent_hashes (deleted parent), verified_hash
\*   crates/query/src/plan/chunk_gate.rs    ChunkGate::allows, ChunkGate::decide (one decision per parent per query)
\*   crates/query/src/planner/builder/mod.rs maybe_wrap_with_acp_filter (a new gate per query)
\*   crates/db/src/chunk/access.rs         history_parents, read_owners, merged (an unarrived parent owns nothing)
\*   crates/p2p-adapter/src/read_gate.rs    chunk block serving; crates/db/src/block/verify.rs block verify
\* Merging text and list across peers is ChunkMerge.tla's job, not this one's.
EXTENDS Naturals, FiniteSets

CONSTANTS
  MaxEdits,      \* how many edits the writers make, at most
  CheckDeleted,  \* TRUE: a deleted article shows no chunks (the guard)
  CheckHash,     \* TRUE: the chunk's text is hashed again (the guard)
  MemoPerQuery,  \* TRUE: each query starts with no remembered decision (the guard)
  CheckMerged    \* TRUE: a block's owner must be an article that has arrived (the guard)

\* Writers edit at least once.
ASSUME MaxEdits \in Nat \ {0}
\* The four switches are yes-or-no.
ASSUME CheckDeleted \in BOOLEAN /\ CheckHash \in BOOLEAN /\ MemoPerQuery \in BOOLEAN
ASSUME CheckMerged \in BOOLEAN

\* The texts an edit may write.
Texts == {"t1", "t2"}

\* The hashes of the chunks the splitter cuts each text into.
ChunksOf(t) == CASE t = "t1" -> {"a", "b"} [] t = "t2" -> {"c"}

\* Every chunk hash there is.
Hashes == UNION {ChunksOf(t) : t \in Texts}

\* A stored chunk: the hash it claims, and whether its text really has that hash.
Stored == [hash : Hashes, honest : BOOLEAN]

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
  leaked    \* whether a peer was ever served a block it may not have

\* All the variables, for stuttering steps.
vars == <<text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked>>

\* The chunks an honest write of text t stores.
HonestChunks(t) == {[hash |-> h, honest |-> TRUE] : h \in ChunksOf(t)}

\* The store's state right now, as a query would see it.
Now == [text |-> text, merged |-> merged, deleted |-> deleted, granted |-> granted,
        stored |-> stored]

\* The decision about the article from view v: may its chunks show, and which.
Decide(v) ==
  [ok |-> (v.merged /\ v.granted /\ (CheckDeleted => ~v.deleted)),  \* arrived, readable, live
   hashes |-> ChunksOf(v.text),                          \* the chunks its text uses
   made |-> TRUE]                                        \* a decision exists now

\* Whether a stored chunk passes the hash check.
HashOK(c) == CheckHash => c.honest

\* Whether serving a chunk's block is allowed: the owner is the article, which
\* with the guard must have arrived. An article that hasn't arrived has no
\* access rules here, so it counts as public.
ServeAllowed ==
  /\ CheckMerged => merged                           \* the guard: the owner is known
  /\ merged => granted                               \* a known article's rules apply

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

\* A writer edits the live article to text t, storing t's chunks with it.
Edit(t) ==
  /\ ~deleted /\ edits < MaxEdits                   \* live, edits left
  /\ text' = t                                      \* the new text
  /\ stored' = stored \cup HonestChunks(t)          \* and its chunks
  /\ edits' = edits + 1                             \* one more edit
  /\ UNCHANGED <<merged, deleted, granted, phase, snap, memo, result, leaked>>

\* A writer deletes the article. Its chunks stay stored.
Delete ==
  /\ ~deleted                                       \* not yet deleted
  /\ deleted' = TRUE                                \* now deleted
  /\ UNCHANGED <<merged, text, granted, stored, edits, phase, snap, memo, result, leaked>>

\* The owner revokes the reader's access.
Revoke ==
  /\ granted                                        \* still granted
  /\ granted' = FALSE                               \* now revoked
  /\ UNCHANGED <<merged, text, deleted, stored, edits, phase, snap, memo, result, leaked>>

\* A chunk that claims hash h but holds other text reaches the store by some
\* route that skipped the write guard.
Plant(h) ==
  /\ stored' = stored \cup {[hash |-> h, honest |-> FALSE]}  \* the forged chunk
  /\ UNCHANGED <<merged, text, deleted, granted, edits, phase, snap, memo, result, leaked>>

\* A query starts: it takes its snapshot, and with the guard forgets old decisions.
Start ==
  /\ phase \in {"idle", "done"}                     \* no query running
  /\ phase' = "reading"                             \* one is now
  /\ snap' = Now                                    \* reading this moment
  /\ memo' = IF MemoPerQuery THEN NoDecision ELSE memo  \* the guard: a fresh gate
  /\ result' = {}                                   \* nothing shown yet
  /\ UNCHANGED <<text, merged, deleted, granted, stored, edits, leaked>>

\* The query shows the chunks the decision allows, deciding first if needed.
Answer ==
  /\ phase = "reading"                              \* a query is running
  /\ LET d == IF memo.made THEN memo ELSE Decide(snap)  \* reuse or decide
     IN /\ memo' = d                                \* remember the decision
        /\ result' = {c \in snap.stored :           \* the stored chunks that
             HashOK(c) /\ d.ok /\ c.hash \in d.hashes}  \* pass every check
  /\ phase' = "done"                                \* the query has answered
  /\ UNCHANGED <<text, merged, deleted, granted, stored, edits, snap, leaked>>

\* The article arrives on this node, with its access rules.
Arrive ==
  /\ ~merged                                        \* not here yet
  /\ merged' = TRUE                                 \* here now
  /\ UNCHANGED <<text, deleted, granted, stored, edits, phase, snap, memo, result, leaked>>

\* A peer that may read only what the reader may asks for a stored chunk's block.
\* It is served when allowed; a serve the peer may not have is a leak.
Serve ==
  /\ stored # {}                                    \* there is a block to ask for
  /\ ServeAllowed                                   \* this node hands it over
  /\ leaked' = (leaked \/ ~(merged /\ granted))     \* bad if the article is unknown or unreadable
  /\ UNCHANGED <<text, merged, deleted, granted, stored, edits, phase, snap, memo, result>>

\* Every step anyone can take.
Next ==
  \/ \E t \in Texts : Edit(t)    \* an edit,
  \/ Delete                      \* a delete,
  \/ Revoke                      \* a revoke,
  \/ \E h \in Hashes : Plant(h)  \* a forged chunk,
  \/ Arrive                      \* the article arrives,
  \/ Serve                       \* a peer is served a block,
  \/ Start                       \* a query starts,
  \/ Answer                      \* or answers

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
====

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
\* The article also stores a list of its chunks' hashes. The article's writer
\* writes that list, and may name in it chunks of old text while its first
\* entry still names the current text. So the decision cuts the current text
\* itself and never trusts the list.
\*
\* An app read validator may also govern the chunk collection itself. It can
\* refuse a reader a chunk the article would let them see, so a chunk shows
\* only when that check passes too.
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
\*     of one of the article's own blocks (INV_DelegatedKeyInScope).
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
  CheckResolved, \* TRUE: nothing is served while the chunk's collections are unknown here (the guard)
  CutText,       \* TRUE: the decision cuts the text, never trusting the stored list (the guard)
  CheckOwnApp,   \* TRUE: the chunk collection's own app check runs too (the guard)
  CheckDelegationScope \* TRUE: a delegation is checked against the collection of the block (the guard)

\* Writers edit at least once.
ASSUME MaxEdits \in Nat \ {0}
\* The four switches are yes-or-no.
ASSUME CheckDeleted \in BOOLEAN /\ CheckHash \in BOOLEAN /\ MemoPerQuery \in BOOLEAN
ASSUME CheckMerged \in BOOLEAN
\* The last two switches are yes-or-no too.
ASSUME CheckGranted \in BOOLEAN /\ CheckListed \in BOOLEAN
\* And the resolve switch.
ASSUME CheckResolved \in BOOLEAN
\* And the cut switch.
ASSUME CutText \in BOOLEAN
\* And the own app check switch.
ASSUME CheckOwnApp \in BOOLEAN
\* And the delegation scope switch.
ASSUME CheckDelegationScope \in BOOLEAN

\* The collections a block can be in, and a delegation bound to: the article's or its chunk collection.
Collections == {"parent", "chunk"}

\* Whether a delegation on collection d covers a block in collection b: one on
\* the article covers every block, one on the chunk collection only its own.
DelegationCovers(d, b) == d = "parent" \/ d = b

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
          stored : SUBSET Stored, padded : BOOLEAN, ownApp : BOOLEAN]

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
  padded,   \* whether the stored list also names every old chunk, its first entry still right
  ownApp,   \* whether the chunk collection's own app check lets the reader read its chunks
  scopeLeaked \* whether a delegated peer ever got a key its delegation doesn't cover

\* All the variables, for stuttering steps.
vars == <<text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked, resolved, padded, ownApp, scopeLeaked>>

\* The chunks an honest write of text t stores.
HonestChunks(t) == {[hash |-> h, honest |-> TRUE] : h \in ChunksOf(t)}

\* The store's state right now, as a query would see it.
Now == [text |-> text, merged |-> merged, deleted |-> deleted, granted |-> granted,
        stored |-> stored, padded |-> padded, ownApp |-> ownApp]

\* The hashes the stored list names: the text's, plus every hash when padded.
ListHashes(v) == ChunksOf(v.text) \cup (IF v.padded THEN Hashes ELSE {})

\* The decision about the article from view v: may its chunks show, and which.
Decide(v) ==
  [ok |-> (v.merged /\ (CheckGranted => v.granted) /\ (CheckDeleted => ~v.deleted)),  \* arrived, readable (with the guard), live
   hashes |-> IF CutText THEN ChunksOf(v.text) ELSE ListHashes(v),  \* the text cut again, or the list
   made |-> TRUE]                                        \* a decision exists now

\* Whether a stored chunk passes the hash check.
HashOK(c) == CheckHash => c.honest

\* Whether decision d lists chunk c. Without the guard every stored chunk counts as listed.
ListedOK(c, d) == CheckListed => c.hash \in d.hashes

\* Whether serving a chunk's block is allowed: the owner is the article, which
\* with the guard must have arrived. An article that hasn't arrived has no
\* access rules here, so it counts as public. The same holds when the node
\* can't find the collections, since then it finds no rules at all.
ServeAllowed ==
  /\ CheckResolved => resolved                       \* the guard: the collections are known
  /\ CheckMerged => merged                           \* the guard: the owner is known
  /\ (resolved /\ merged) => granted                 \* a known article's rules apply

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
  /\ padded \in BOOLEAN                               \* list padded or not
  /\ ownApp \in BOOLEAN                               \* own app check passes or not
  /\ scopeLeaked \in BOOLEAN                          \* an out of scope key went out

\* At the start the chunks of t1 are here but the article hasn't arrived yet,
\* and the reader may read the article.
Init ==
  /\ text = "t1"                           \* first text
  /\ merged = FALSE                        \* the article is still on its way
  /\ deleted = FALSE                       \* live
  /\ granted = TRUE                        \* readable
  /\ padded = FALSE                        \* the list names only the text's chunks
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

\* A writer edits the live article to text t, storing t's chunks with it.
Edit(t) ==
  /\ ~deleted /\ edits < MaxEdits                   \* live, edits left
  /\ text' = t                                      \* the new text
  /\ stored' = stored \cup HonestChunks(t)          \* and its chunks
  /\ edits' = edits + 1                             \* one more edit
  /\ padded' = FALSE                                \* with an honest list
  /\ UNCHANGED <<scopeLeaked, merged, deleted, granted, phase, snap, memo, result, leaked, resolved, ownApp>>

\* A writer deletes the article. Its chunks stay stored.
Delete ==
  /\ ~deleted                                       \* not yet deleted
  /\ deleted' = TRUE                                \* now deleted
  /\ UNCHANGED <<scopeLeaked, merged, text, granted, stored, edits, phase, snap, memo, result, leaked, resolved, padded, ownApp>>

\* The owner revokes the reader's access.
Revoke ==
  /\ granted                                        \* still granted
  /\ granted' = FALSE                               \* now revoked
  /\ UNCHANGED <<scopeLeaked, merged, text, deleted, stored, edits, phase, snap, memo, result, leaked, resolved, padded, ownApp>>

\* A chunk that claims hash h but holds other text reaches the store by some
\* route that skipped the write guard.
Plant(h) ==
  /\ stored' = stored \cup {[hash |-> h, honest |-> FALSE]}  \* the forged chunk
  /\ UNCHANGED <<scopeLeaked, merged, text, deleted, granted, edits, phase, snap, memo, result, leaked, resolved, padded, ownApp>>

\* The article's writer stores a list that names every old chunk too, its
\* first entry still the current text's hash.
Pad ==
  /\ ~padded                                        \* not padded yet
  /\ padded' = TRUE                                 \* padded now
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked, resolved, ownApp>>

\* The chunk collection's own app check starts refusing the reader.
AppRefuse ==
  /\ ownApp                                        \* still passing
  /\ ownApp' = FALSE                               \* refusing now
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked, resolved, padded>>

\* A query starts: it takes its snapshot, and with the guard forgets old decisions.
Start ==
  /\ phase \in {"idle", "done"}                     \* no query running
  /\ phase' = "reading"                             \* one is now
  /\ snap' = Now                                    \* reading this moment
  /\ memo' = IF MemoPerQuery THEN NoDecision ELSE memo  \* the guard: a fresh gate
  /\ result' = {}                                   \* nothing shown yet
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, leaked, resolved, padded, ownApp>>

\* The query shows the chunks the decision allows, deciding first if needed.
Answer ==
  /\ phase = "reading"                              \* a query is running
  /\ LET d == IF memo.made THEN memo ELSE Decide(snap)  \* reuse or decide
     IN /\ memo' = d                                \* remember the decision
        /\ result' = {c \in snap.stored :           \* the stored chunks that
             HashOK(c) /\ d.ok /\ ListedOK(c, d)   \* pass every check on the article
             /\ (CheckOwnApp => snap.ownApp)}       \* and, with the guard, the own app check
  /\ phase' = "done"                                \* the query has answered
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, snap, leaked, resolved, padded, ownApp>>

\* The article arrives on this node, with its access rules.
Arrive ==
  /\ ~merged                                        \* not here yet
  /\ merged' = TRUE                                 \* here now
  /\ UNCHANGED <<scopeLeaked, text, deleted, granted, stored, edits, phase, snap, memo, result, leaked, resolved, padded, ownApp>>

\* The chunk collection and the article collection that chunks it become known here.
Resolve ==
  /\ ~resolved                                      \* not known yet
  /\ resolved' = TRUE                               \* known now
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked, padded, ownApp>>

\* A peer that may read only what the reader may asks for a stored chunk's block.
\* It is served when allowed; a serve the peer may not have is a leak.
Serve ==
  /\ stored # {}                                    \* there is a block to ask for
  /\ ServeAllowed                                   \* this node hands it over
  /\ leaked' = (leaked \/ ~(merged /\ granted))     \* bad if the article is unknown or unreadable
  /\ UNCHANGED <<scopeLeaked, text, merged, deleted, granted, stored, edits, phase, snap, memo, result, resolved, padded, ownApp>>

\* A peer replaying for the reader under a delegation on collection d asks for
\* the key of a block in collection b. The key follows the article's rules, as
\* serving a block does. Without the guard, any of the article's collections
\* counts as covering every block of the article.
DelegatedKey(d, b) ==
  /\ ServeAllowed                                    \* the article's rules let the key go
  /\ CheckDelegationScope => DelegationCovers(d, b)  \* the guard: the delegation covers the block
  /\ scopeLeaked' = (scopeLeaked \/ ~DelegationCovers(d, b))  \* bad if it doesn't
  /\ UNCHANGED <<text, merged, deleted, granted, stored, edits, phase, snap, memo, result, leaked, resolved, padded, ownApp>>

\* Every step anyone can take.
Next ==
  \/ \E t \in Texts : Edit(t)    \* an edit,
  \/ Delete                      \* a delete,
  \/ Revoke                      \* a revoke,
  \/ \E h \in Hashes : Plant(h)  \* a forged chunk,
  \/ Arrive                      \* the article arrives,
  \/ Resolve                     \* the collections become known,
  \/ Pad                         \* the list is padded,
  \/ AppRefuse                   \* the own app check refuses,
  \/ Serve                       \* a peer is served a block,
  \/ \E d, b \in Collections : DelegatedKey(d, b)  \* a delegated peer asks for a key,
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
\* A shown chunk passed the chunk collection's own app check at the query's snapshot.
INV_ShownPassesOwnApp == (phase = "done" /\ result # {}) => snap.ownApp

\* A peer is served a chunk's block only for an article that has arrived and that it may read.
INV_ServedIsReadable == ~leaked

\* A peer replaying under a delegation gets only keys of blocks its delegation covers.
INV_DelegatedKeyInScope == ~scopeLeaked
====

---- MODULE ChunkCleanup ----
\* The story: an article with a `@chunk` text lives on two peers. Each peer
\* edits it on its own, and the peers swap edits and chunk documents later, in
\* any order. A chunk document belongs to one article: it names the article
\* (its _sourceID), the chunk's hash, and the edit of the text it was first
\* cut from (its sourceCID). Those three give the document its ID.
\*
\* Every edit is a commit of the text that names the commits it was made on,
\* so the edits form a history, and a peer merges an edit only after the ones
\* it was made on. The commits no other commit names are the heads. The text
\* that shows is the winner's: the highest commit, then the bigger text, then
\* the bigger commit, the same way on every peer.
\*
\* An edit writes the text, deletes the chunk documents the new text does not
\* use, and creates the ones it lacks, all in one transaction. A chunk it
\* keeps keeps its document; a chunk it makes names the edit. A merge that
\* leaves one head does the same for the winning text. A merge that leaves
\* two heads (edits made apart) moves every chunk to the winner: chunks that
\* name another commit go, and the winner's are made. Both rules depend only
\* on the history, so two peers with the same edits hold the same chunk
\* documents. A deleted document's ID can never be used again, and a winner
\* never comes back once beaten, so no rule needs a deleted document again.
\* Chunk blocks are not signed, so two peers that make the same chunk
\* document give it the same ID.
\*
\* Each chunk create and each chunk delete can reach the other peer at any
\* time. A chunk that names an edit not merged here yet waits for it: it is
\* stored, no rule here deletes it, and a delete of it waits. Otherwise a
\* create whose chunk the text here does not use, or already holds, is
\* dropped, and a later merge makes it if it is needed. A delete waits while
\* the article here has not arrived, or its text uses the chunk, and merges
\* otherwise. A delete of a chunk not stored here is dropped and marks
\* nothing, so a later merge here can still make that chunk.
\*
\* What must never happen:
\*   - a live article's text uses a chunk with no live document here, so a
\*     reader misses it (INV_VisibleIsSplit);
\*   - a peer holds a live document for a chunk its text does not use, so
\*     storage grows with every edit (INV_StoredIsSplit);
\*   - a chunk the text uses is held by two live documents (INV_EachChunkOnce);
\*   - a deleted document comes back to life (INV_NoResurrect);
\*   - generations grow without an edit to cause them, a loop (INV_GenerationsBounded);
\*   - peers that have every edit hold different chunk documents (INV_SettledSameIds).
\* The first three ignore a chunk that waits for its edit: it is on its way.
\*
\* Two bugs are kept as switches to show what the fix prevents: picking a
\* chunk's ID from what this peer deleted ("generation", the code before
\* sourceCID), and one fixed ID per chunk ("none").
\*
\* Code it models:
\*   crates/database/db/src/chunk/sync/mod.rs      ChunkWrites::sync_chunks (an edit's deletes and creates)
\*   crates/database/db/src/chunk/sync/lookup.rs   ChunkCollection::stored_of, settle, waits (live chunks by hash, one kept per hash)
\*   crates/database/db/src/chunk/sync/write.rs    ChunkWrites::create (a new chunk names the edit)
\*   crates/database/db/src/chunk/source.rs        winning_commit, is_merged (the commit a chunk names, and whether it merged here)
\*   crates/database/db/src/chunk/rebuild.rs       reconcile (a merge's deletes and creates, one head or two)
\*   crates/database/db/src/chunk/peer_delete.rs   delete_verdict, parent_state (wait while the text uses the chunk, drop when not stored)
\*   crates/database/db/src/chunk/peer_create.rs   create_is_stale (drop a create the text does not use)
\*   crates/database/db/src/merge/merge_handler/composite_persist.rs  where the merge calls them
\* Encrypted chunks take a key of their own, and embedded ones a vector only
\* the writer's model gives, so a merge builds neither; that case is not
\* modelled here.
\* Numbers, records and finite sets are all this model needs.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Peers,        \* the two peers, for example {p1, p2}
  MaxEdits,     \* how many edits each peer makes, at most
  CleanWrites,  \* TRUE: an edit deletes the chunks its text does not use (the fix)
  CleanMerges,  \* TRUE: a merge deletes the chunks the winning text does not use (the fix)
  DropStale,    \* TRUE: a create the text here does not use, or already holds, is dropped (the fix)
  DeleteGuard,  \* TRUE: a delete waits while the text here uses its chunk (the fix)
  Ids,          \* how a new chunk gets its ID: "source" (the fix), "generation" or "none"
  Reanchor,     \* TRUE: a merge that leaves two heads moves every chunk to the winner (the fix)
  KeepWaiting   \* TRUE: a chunk that names an edit not merged here yet is left alone (the fix)

\* Each peer edits at least once.
ASSUME MaxEdits \in Nat \ {0}
\* The cleanup switches are yes-or-no.
ASSUME CleanWrites \in BOOLEAN /\ CleanMerges \in BOOLEAN
\* So are the guards.
ASSUME DropStale \in BOOLEAN /\ DeleteGuard \in BOOLEAN /\ Reanchor \in BOOLEAN
\* And so is leaving waiting chunks alone.
ASSUME KeepWaiting \in BOOLEAN
\* The three ways a chunk can be named.
ASSUME Ids \in {"source", "generation", "none"}

\* The texts an edit may write. Changing t1 to t2 and back is a revert.
Texts == {"t1", "t2"}

\* The chunks each text is cut into. "a" is shared, so an edit keeps it.
ChunksOf(t) == CASE t = "t1" -> {"a", "b"}
                 [] t = "t2" -> {"a", "c"}

\* How two texts compare in a tie: the bytes of the text.
TextRank(t) == CASE t = "t1" -> 1 [] t = "t2" -> 2

\* Every chunk hash any text has.
Hashes == UNION {ChunksOf(t) : t \in Texts}

\* The most edits there can be, so the numbers that name them.
MaxId == MaxEdits * Cardinality(Peers)

\* The most generations a chunk can reach: one per edit is plenty.
MaxGen == 2 * MaxId

\* A chunk document: its hash, the edit it names (0 for none), and its
\* generation (0 unless Ids = "generation"). Its ID is these three.
Docs == Hashes \X (0..MaxId) \X (0..MaxGen)

\* An edit: its number, who made it, at which height, the text it wrote, and
\* the numbers of the edits it was made on.
Edits == [id : 1..MaxId, peer : Peers, height : 1..MaxId, text : Texts, parents : SUBSET (1..MaxId)]

VARIABLES
  made,   \* every edit made so far, by anyone
  known,  \* known[p]: the edits peer p has merged
  count,  \* count[p]: how many edits peer p has made
  live,   \* live[p]: the chunk documents live on peer p
  dead,   \* dead[p]: the chunk documents deleted on peer p
  back    \* back[p]: a deleted document of p was made live again

\* All the variables, for stuttering steps.
vars == <<made, known, count, live, dead, back>>

\* The numbers of a set of edits.
IdsOf(es) == {e.id : e \in es}

\* The heads of a set of edits: the ones no other edit there was made on.
Heads(es) == {e \in es : \A f \in es : e.id \notin f.parents}

\* Whether edit e beats edit f: higher, or as high with a bigger text, or the
\* same text with a bigger number (the CID in the code).
Beats(e, f) ==
  \/ f.height < e.height                                               \* f is lower,
  \/ f.height = e.height /\ TextRank(f.text) < TextRank(e.text)         \* or as high with a smaller text,
  \/ f.height = e.height /\ f.text = e.text /\ f.id <= e.id             \* or the same text and not a bigger number

\* The edit among `es` that wins.
Winner(es) == CHOOSE e \in es : \A f \in es : Beats(e, f)

\* The text a set of edits leaves.
TextOf(es) == Winner(es).text

\* The text peer p holds.
TextOn(p) == TextOf(known[p])

\* The hashes of a set of documents.
HashesOf(ds) == {d[1] : d \in ds}

\* The documents of hash h in a set.
OfHash(ds, h) == {d \in ds : d[1] = h}

\* Whether document d is on its way: it names an edit the edits `es` lack.
Pending(es, d) == d[2] # 0 /\ d[2] \notin IdsOf(es)

\* The documents of `ds` that are not on their way, given the edits `es`.
Arrived(es, ds) == {d \in ds : ~Pending(es, d)}

\* Whether the rules let d wait for its edit. Without the fix nothing waits,
\* so every rule treats it like any other document.
Waiting(es, d) == KeepWaiting /\ Pending(es, d)

\* The documents of `ds` the rules treat as ready, given the edits `es`.
Ready(es, ds) == {d \in ds : ~Waiting(es, d)}

\* How documents of one hash are ordered when one must be kept: every peer
\* compares them the same way (the code compares DocIDs).
Rank(d) == d[2] * (MaxGen + 1) + d[3]

\* The generation a peer with deleted documents `gone` gives a new chunk h:
\* the first one not deleted there. Only the "generation" bug uses it.
FreeGen(gone, h) ==
  CHOOSE g \in 0..MaxGen : <<h, 0, g>> \notin gone /\ \A k \in 0..(g - 1) : <<h, 0, k>> \in gone

\* The document a step makes for chunk h whose winning edit is w.
NewDoc(gone, h, w) ==
  CASE Ids = "source" -> <<h, w.id, 0>>                 \* the fix: it names the edit
    [] Ids = "generation" -> <<h, 0, FreeGen(gone, h)>> \* bug: the first ID unused here
    [] Ids = "none" -> <<h, 0, 0>>                     \* bug: always the same ID

\* What a step that leaves the edits `es` with one head keeps of `ls`: every
\* waiting document, and of the others, with `clean`, only those the text
\* uses, one per chunk (without it, all of them).
KeptOneHead(es, ls, clean) ==
  LET t == TextOf(es)                                   \* the winning text
      done == Ready(es, ls)                             \* documents not waiting
  IN (ls \ done) \cup                                   \* waiting ones stay
     IF clean
     THEN {d \in done : d[1] \in ChunksOf(t) /\ \A e \in OfHash(done, d[1]) : Rank(e) <= Rank(d)}
     ELSE done                                          \* without the fix, all stay

\* The live documents after a step that leaves the edits `es` on a peer that
\* holds `ls` live and `gone` deleted. With one head (or without the fix) it
\* keeps what KeptOneHead keeps and makes the chunks still missing. With two
\* heads it keeps the waiting documents and the winner's, and makes the rest
\* of the winner's.
After(es, ls, gone, clean) ==
  LET t == TextOf(es)                                   \* the winning text
      w == Winner(es)                                   \* and its edit
      done == Ready(es, ls)                             \* documents not waiting
  IN IF Cardinality(Heads(es)) > 1 /\ Reanchor /\ Ids = "source"
     THEN (ls \ done)                                   \* waiting ones stay,
          \cup {<<h, w.id, 0>> : h \in ChunksOf(t)}     \* the winner's are live,
          \cup (IF clean THEN {} ELSE done)             \* and without the fix nothing goes
     ELSE LET keep == KeptOneHead(es, ls, clean)        \* what stays
          IN keep \cup {NewDoc(gone, h, w) : h \in ChunksOf(t) \ HashesOf(Ready(es, keep))}  \* and what is made

\* The heights an edit may use: one above what the peer has merged.
NextHeight(p) == IF known[p] = {} THEN 1 ELSE 1 + Winner(known[p]).height

\* The type of every variable.
TypeOK ==
  /\ made \subseteq Edits                         \* edits are edits
  /\ known \in [Peers -> SUBSET Edits]             \* each peer knows some
  /\ count \in [Peers -> 0..MaxEdits]              \* edits made, per peer
  /\ live \in [Peers -> SUBSET Docs]               \* live documents are chunk documents
  /\ dead \in [Peers -> SUBSET Docs]               \* and so are deleted ones
  /\ back \in [Peers -> BOOLEAN]                   \* a resurrection happened or not

\* At the start no one has edited anything, and no chunk exists.
Init ==
  /\ made = {}                          \* no edits yet
  /\ known = [p \in Peers |-> {}]        \* no peer knows any
  /\ count = [p \in Peers |-> 0]         \* no peer has edited
  /\ live = [p \in Peers |-> {}]         \* nothing live
  /\ dead = [p \in Peers |-> {}]         \* nothing deleted
  /\ back = [p \in Peers |-> FALSE]      \* nothing came back

\* Peer p moves to the edits `es`, which leave its live documents as `now`:
\* the ones that go are deleted, and making one deleted before resurrects it.
Move(p, es, now) ==
  /\ known' = [known EXCEPT ![p] = es]                         \* the edits it holds
  /\ live' = [live EXCEPT ![p] = now]                          \* the new live set
  /\ dead' = [dead EXCEPT ![p] = (@ \cup (live[p] \ now)) \ now] \* the deletes
  /\ back' = [back EXCEPT ![p] = @ \/ ((now \ live[p]) \cap dead[p] # {})]  \* a deleted ID made again

\* Peer p edits the article to text t. One transaction writes the text,
\* deletes the documents t does not use, and makes the ones it lacks.
Edit(p, t) ==
  /\ count[p] < MaxEdits                                       \* p has edits left
  /\ LET e == [id |-> Cardinality(made) + 1, peer |-> p, height |-> NextHeight(p),
               text |-> t, parents |-> IdsOf(Heads(known[p]))] \* the new edit, on p's heads
         es == known[p] \cup {e}                                \* what p holds after it
     IN /\ made' = made \cup {e}                                 \* the edit exists
        /\ Move(p, es, After(es, live[p], dead[p], CleanWrites)) \* and p's chunks follow it
  /\ count' = [count EXCEPT ![p] = @ + 1]                       \* one more edit by p

\* Peer p merges an edit another peer made, once it holds the edits that one
\* was made on. The merge's transaction deletes and makes chunks for the text
\* that wins.
MergeEdit(p) ==
  \E e \in made \ known[p] :                           \* an edit p lacks
    /\ e.parents \subseteq IdsOf(known[p])               \* whose history p has
    /\ LET es == known[p] \cup {e}                       \* what p knows after it
       IN Move(p, es, After(es, live[p], dead[p], CleanMerges))  \* its chunks follow
    /\ UNCHANGED <<made, count>>                         \* no edit is made

\* Whether a create of d that reaches p is dropped: d waits for no edit, the
\* article is here, and its text does not use d or p already holds a live
\* document for d's chunk.
Stale(p, d) ==
  /\ DropStale                                          \* the fix is on
  /\ ~Waiting(known[p], d)                              \* d names an edit p merged, or none
  /\ known[p] # {}                                      \* the article is here
  /\ \/ d[1] \notin ChunksOf(TextOn(p))                 \* unused,
     \/ Ready(known[p], OfHash(live[p], d[1])) # {}     \* or held

\* A chunk document's create reaches p from a peer that made it. A document p
\* already deleted stays deleted; a stale one is dropped.
MergeCreate(p) ==
  \E q \in Peers \ {p} :                               \* another peer q
    \E d \in (live[q] \cup dead[q]) \ (live[p] \cup dead[p]) : \* made a document p lacks
      /\ ~Stale(p, d)                                    \* that p keeps
      /\ live' = [live EXCEPT ![p] = @ \cup {d}]         \* it is live on p
      /\ UNCHANGED <<made, known, count, dead, back>>    \* nothing else changes

\* Whether p holds back a delete of d: the article is not here, d waits for
\* its edit, or the text here uses d's chunk.
Waits(p, d) ==
  /\ DeleteGuard                                        \* the fix is on
  /\ \/ known[p] = {}                                   \* the article is not here,
     \/ Waiting(known[p], d)                            \* or d waits for its edit,
     \/ d[1] \in ChunksOf(TextOn(p))                    \* or the text uses d's chunk

\* A delete of d reaches p from a peer that deleted it. A live d goes unless
\* p holds it back. A delete of a d not live here is dropped and marks
\* nothing: p may still make d later, under the same ID.
MergeDelete(p) ==
  \E q \in Peers \ {p} :                               \* another peer q
    \E d \in dead[q] \cap live[p] :                     \* deleted a document live on p
      /\ ~Waits(p, d)                                    \* and p does not hold it back
      /\ live' = [live EXCEPT ![p] = @ \ {d}]            \* it is gone on p
      /\ dead' = [dead EXCEPT ![p] = @ \cup {d}]         \* and deleted there
      /\ UNCHANGED <<made, known, count, back>>          \* nothing else changes

\* Every step a peer can take.
Next ==
  \E p \in Peers :
    \/ \E t \in Texts : Edit(p, t)  \* p edits the article,
    \/ MergeEdit(p)                 \* merges an edit,
    \/ MergeCreate(p)               \* receives a chunk create,
    \/ MergeDelete(p)               \* or a chunk delete

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* A live article's text has a live document for every chunk it uses.
INV_VisibleIsSplit ==
  \A p \in Peers : known[p] # {} =>                         \* per peer holding the article
    ChunksOf(TextOn(p)) \subseteq HashesOf(Arrived(known[p], live[p]))  \* each chunk is held

\* A live article's peer holds live documents only for chunks its text uses.
INV_StoredIsSplit ==
  \A p \in Peers : known[p] # {} =>                         \* per peer holding the article
    HashesOf(Arrived(known[p], live[p])) \subseteq ChunksOf(TextOn(p))  \* nothing extra

\* Each chunk the text uses is held by one live document.
INV_EachChunkOnce ==
  \A p \in Peers : known[p] # {} =>                         \* per peer holding the article
    \A h \in ChunksOf(TextOn(p)) :                          \* each chunk of the text
      Cardinality(OfHash(Arrived(known[p], live[p]), h)) <= 1 \* has at most one document

\* No deleted document is made live again, and none is live and deleted at once.
INV_NoResurrect == \A p \in Peers : ~back[p] /\ live[p] \cap dead[p] = {}  \* per peer

\* A generation is never higher than the number of edits made: each new
\* generation needs an edit that stopped using the chunk.
INV_GenerationsBounded ==
  \A p \in Peers : \A d \in live[p] \cup dead[p] : d[3] <= Cardinality(made)  \* per document

\* Every peer has every edit.
AllMerged == made # {} /\ \A p \in Peers : known[p] = made

\* Peers that have every edit hold the same live chunk documents, ID for ID.
INV_SettledSameIds ==
  AllMerged => \A p, q \in Peers : live[p] = live[q]  \* any two peers
====

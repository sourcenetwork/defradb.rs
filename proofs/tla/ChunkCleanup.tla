---- MODULE ChunkCleanup ----
\* The story: an article with a `@chunk` text lives on two peers. Each peer
\* edits it on its own, and the peers swap edits and chunk documents later, in
\* any order. A chunk document belongs to one article: it names the article
\* (its _sourceID), the chunk's hash, and a generation number.
\*
\* An edit writes the text, deletes the chunk documents the new text does not
\* use, and creates the ones it lacks, all in one transaction. A merge of
\* another peer's edit does the same for the text that wins the merge, in the
\* merge's own transaction. A deleted document's ID can never be used again,
\* so a chunk made again after a delete takes the next generation, which gives
\* it a new ID. Chunk blocks are not signed, so two peers that make the same
\* generation of a chunk give it the same ID.
\*
\* Each chunk create and each chunk delete can reach the other peer at any
\* time. A delete waits while the article here is live and its text uses the
\* chunk; otherwise it merges. A create that arrives for an article here whose
\* text does not use the chunk, or that already holds the chunk, is dropped:
\* if a later text uses the chunk, the merge of that text makes it.
\*
\* What must never happen:
\*   - a live article's text uses a chunk with no live document here, so a
\*     reader misses it (INV_VisibleIsSplit);
\*   - a peer holds a live document for a chunk its text does not use, so
\*     storage grows with every edit (INV_StoredIsSplit);
\*   - a chunk the text uses is held by two live documents (INV_EachChunkOnce);
\*   - a deleted document comes back to life (INV_NoResurrect);
\*   - generations grow without an edit to cause them, a loop (INV_GenerationsBounded);
\*   - peers that have every edit hold different chunks (INV_SettledAgree).
\*
\* Code it models:
\*   crates/db/src/chunk/sync/mod.rs      ChunkWrites::sync_chunks (an edit's deletes and creates)
\*   crates/db/src/chunk/sync/lookup.rs   ChunkCollection::stored_of, unneeded (live chunks by hash, one kept per hash)
\*   crates/db/src/chunk/sync/write.rs    ChunkWrites::create (the next free generation)
\*   crates/db/src/chunk/generation.rs    is_taken, next_generation (skip the IDs used here)
\*   crates/db/src/chunk/rebuild.rs       reconcile (a merge's deletes and creates)
\*   crates/db/src/chunk/peer_delete.rs   delete_must_wait, parent_state (wait while the text uses the chunk)
\*   crates/db/src/chunk/peer_create.rs   create_is_stale (drop a create the text does not use)
\*   crates/db/src/merge/merge_handler/composite_persist.rs  where the merge calls them
\* Encrypted chunks take a key of their own, so their IDs differ per peer and a
\* merge neither builds nor drops them; that case is not modelled here.
\* Numbers and finite sets are all this model needs.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Peers,        \* the two peers, for example {p1, p2}
  MaxEdits,     \* how many edits each peer makes, at most
  CleanWrites,  \* TRUE: an edit deletes the chunks its text does not use (the fix)
  CleanMerges,  \* TRUE: a merge deletes the chunks the winning text does not use (the fix)
  DropStale,    \* TRUE: a create the text here does not use, or already holds, is dropped (the fix)
  DeleteGuard,  \* TRUE: a delete waits while the text here uses the chunk (the fix)
  NextGen       \* TRUE: a chunk made again after a delete takes the next generation (the fix)

\* Each peer edits at least once.
ASSUME MaxEdits \in Nat \ {0}
\* The switches are yes-or-no.
ASSUME CleanWrites \in BOOLEAN /\ CleanMerges \in BOOLEAN
\* So are these.
ASSUME DropStale \in BOOLEAN /\ DeleteGuard \in BOOLEAN /\ NextGen \in BOOLEAN

\* The texts an edit may write. Changing t1 to t2 and back is a revert.
Texts == {"t1", "t2"}

\* The chunks each text is cut into. "a" is shared, so an edit keeps it.
ChunksOf(t) == CASE t = "t1" -> {"a", "b"}
                 [] t = "t2" -> {"a", "c"}

\* How two texts compare in a tie: the bytes of the text.
TextRank(t) == CASE t = "t1" -> 1 [] t = "t2" -> 2

\* Every chunk hash any text has.
Hashes == UNION {ChunksOf(t) : t \in Texts}

\* The most generations a chunk can reach: one per edit is plenty.
MaxGen == 2 * MaxEdits * Cardinality(Peers)

\* A chunk document: its hash and its generation. Its ID is these two.
Docs == Hashes \X (0..MaxGen)

\* An edit: who made it, at which height, and the text it wrote.
Edits == [peer : Peers, height : 1..(2 * MaxEdits), text : Texts]

VARIABLES
  made,   \* every edit made so far, by anyone
  known,  \* known[p]: the edits peer p has merged
  count,  \* count[p]: how many edits peer p has made
  live,   \* live[p]: the chunk documents live on peer p
  dead,   \* dead[p]: the chunk documents deleted on peer p
  back    \* back[p]: a deleted document of p was made live again

\* All the variables, for stuttering steps.
vars == <<made, known, count, live, dead, back>>

\* The edit among `es` that wins: the highest height, then the higher text.
Winner(es) ==
  CHOOSE e \in es : \A f \in es :                              \* e beats every f:
    \/ f.height < e.height                                      \* f is lower,
    \/ (f.height = e.height /\ TextRank(f.text) <= TextRank(e.text)) \* or as high and not bigger

\* The text a set of edits leaves.
TextOf(es) == Winner(es).text

\* The text peer p holds.
TextOn(p) == TextOf(known[p])

\* The hashes of a set of documents.
HashesOf(ds) == {d[1] : d \in ds}

\* The live documents of hash h in a set.
OfHash(ds, h) == {d \in ds : d[1] = h}

\* The generation a peer with deleted documents `gone` gives a new chunk h:
\* the first one not deleted there. Without the fix, always generation 0.
FreeGen(gone, h) ==
  IF NextGen
  THEN CHOOSE g \in 0..MaxGen : <<h, g>> \notin gone /\ \A k \in 0..(g - 1) : <<h, k>> \in gone
  ELSE 0

\* The documents a transaction makes for text t on a peer that holds `ls` live
\* and `gone` deleted: one per chunk of t that has no live document.
Made(ls, gone, t) == {<<h, FreeGen(gone, h)>> : h \in ChunksOf(t) \ HashesOf(ls)}

\* What a transaction that leaves text t keeps live: with `clean`, only
\* documents t uses, and of those, one per chunk, the highest generation.
Kept(ls, t, clean) ==
  IF clean
  THEN {d \in ls : d[1] \in ChunksOf(t) /\ \A e \in OfHash(ls, d[1]) : e[2] <= d[2]}
  ELSE ls

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

\* The documents a step on p makes, minus any that were deleted there before:
\* making one of those again would resurrect it.
Resurrects(p, ds) == ds \cap dead[p] # {}

\* Peer p edits the article to text t. One transaction writes the text,
\* deletes the documents t does not use, and makes the ones it lacks.
Edit(p, t) ==
  /\ count[p] < MaxEdits                                       \* p has edits left
  /\ LET e == [peer |-> p, height |-> NextHeight(p), text |-> t] \* the new edit
         keep == Kept(live[p], t, CleanWrites)                  \* what stays live
         new == Made(keep, dead[p], t)                          \* what is made
     IN /\ made' = made \cup {e}                                 \* the edit exists
        /\ known' = [known EXCEPT ![p] = @ \cup {e}]             \* and p has it
        /\ live' = [live EXCEPT ![p] = keep \cup new]            \* the new live set
        /\ dead' = [dead EXCEPT ![p] = (@ \cup (live[p] \ keep)) \ new] \* the deletes
        /\ back' = [back EXCEPT ![p] = @ \/ Resurrects(p, new)]  \* a made-again ID resurrects
  /\ count' = [count EXCEPT ![p] = @ + 1]                       \* one more edit by p

\* Peer p merges an edit another peer made. The text that wins is cut, and
\* the merge's transaction deletes and makes chunks for it like an edit does.
MergeEdit(p) ==
  \E e \in made \ known[p] :                           \* an edit p lacks
    LET k == known[p] \cup {e}                           \* what p knows after it
        t == TextOf(k)                                   \* the text that wins
        keep == Kept(live[p], t, CleanMerges)            \* what stays live
        new == Made(keep, dead[p], t)                    \* what the merge makes
    IN /\ known' = [known EXCEPT ![p] = k]               \* the edit is merged
       /\ live' = [live EXCEPT ![p] = keep \cup new]     \* the new live set
       /\ dead' = [dead EXCEPT ![p] = (@ \cup (live[p] \ keep)) \ new] \* the merge's deletes
       /\ back' = [back EXCEPT ![p] = @ \/ Resurrects(p, new)]         \* a made-again ID resurrects
       /\ UNCHANGED <<made, count>>                      \* no edit is made

\* Whether a create of d that reaches p is dropped: the article is here and its
\* text does not use d, or p already holds a live document for d's chunk.
Stale(p, d) ==
  /\ DropStale                                          \* the fix is on
  /\ known[p] # {}                                      \* the article is here
  /\ (d[1] \notin ChunksOf(TextOn(p)) \/ OfHash(live[p], d[1]) # {})  \* unused or held

\* A chunk document's create reaches p from a peer that made it. A document p
\* already deleted stays deleted; a stale one is dropped.
MergeCreate(p) ==
  \E q \in Peers \ {p} :                               \* another peer q
    \E d \in (live[q] \cup dead[q]) \ (live[p] \cup dead[p]) : \* made a document p lacks
      /\ ~Stale(p, d)                                    \* that p keeps
      /\ live' = [live EXCEPT ![p] = @ \cup {d}]         \* it is live on p
      /\ UNCHANGED <<made, known, count, dead, back>>    \* nothing else changes

\* Whether p holds back a delete of d: its live article's text uses d's chunk.
Waits(p, d) ==
  /\ DeleteGuard                                        \* the fix is on
  /\ known[p] = {} \/ d[1] \in ChunksOf(TextOn(p))      \* the article is not here, or uses d

\* A delete of d reaches p from a peer that deleted it.
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
  \A p \in Peers : known[p] # {} => ChunksOf(TextOn(p)) \subseteq HashesOf(live[p])  \* per peer

\* A live article's peer holds live documents only for chunks its text uses.
INV_StoredIsSplit ==
  \A p \in Peers : known[p] # {} => HashesOf(live[p]) \subseteq ChunksOf(TextOn(p))  \* per peer

\* Each chunk the text uses is held by one live document.
INV_EachChunkOnce ==
  \A p \in Peers : known[p] # {} =>                         \* per peer holding the article
    \A h \in ChunksOf(TextOn(p)) : Cardinality(OfHash(live[p], h)) <= 1  \* at most one each

\* No deleted document is made live again, and none is live and deleted at once.
INV_NoResurrect == \A p \in Peers : ~back[p] /\ live[p] \cap dead[p] = {}  \* per peer

\* A generation is never higher than the number of edits made: each new
\* generation needs an edit that stopped using the chunk.
INV_GenerationsBounded ==
  \A p \in Peers : \A d \in live[p] \cup dead[p] : d[2] <= Cardinality(made)  \* per document

\* Every peer has every edit.
Settled == made # {} /\ \A p \in Peers : known[p] = made

\* Peers that have every edit hold live documents for the same chunks: those
\* of the text.
INV_SettledAgree ==
  Settled => \A p, q \in Peers : HashesOf(live[p]) = HashesOf(live[q])  \* any two peers
====

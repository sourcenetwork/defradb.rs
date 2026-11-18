---- MODULE ChunkMerge ----
\* The story: an article with a `@chunk` text lives on two peers. Each peer
\* edits it on its own and the peers swap their edits later. An edit sets the
\* text and the chunk list in one write, and creates the chunks of the new text
\* on the peer that made it. Chunks travel to the other peer as documents of
\* their own, at any time.
\*
\* The text and the list merge as two separate fields, each "last writer wins":
\* the edit with the higher height wins, and two edits at the same height are
\* broken by comparing the two values. The text and the list compare different
\* bytes, so a tie can keep one peer's text and the other peer's list.
\*
\* A reader shows the chunks of the current text: it cuts the text itself and
\* never trusts the list, because the list can come from another edit.
\*
\* A peer can also send a chunk document that lies. One lie is a chunk whose
\* hash is not the hash of its text. Another is a new version of a chunk the
\* receiver already stores, which would change it after it was created. The
\* merge guard refuses both before anything is stored.
\*
\* A peer may delete the article. That delete also deletes every chunk it
\* stores, and each chunk delete travels on its own, so a chunk delete can reach
\* a peer before the article's delete. A peer can also send a chunk delete no
\* article delete made. A delete names its chunk only through the chunk's first
\* block, so it can't merge on a peer before that block has merged there. It
\* waits while the article here is live or has not arrived, and merges once
\* the article is gone. A chunk the text does not use now may be used again by
\* a later edit, so it waits too.
\*
\* What must never happen:
\*   - a reader shows a chunk that is not part of the current text (INV_ReadsMatchText);
\*   - once every edit and chunk has reached both peers, they show different
\*     chunks, or miss a chunk of the text (INV_SettledPeersAgree);
\*   - a peer stores a chunk whose hash does not match its text (INV_NoForgedChunk);
\*   - a stored chunk is changed after it was created (INV_ChunksNeverChange);
\*   - a live article loses a chunk its text uses (INV_ListedChunksStay);
\*   - a peer that holds the article's delete refuses one of its chunk deletes
\*     for good, so it keeps chunks the other peers deleted (INV_DeletesNotLost).
\*
\* Code it models:
\*   crates/query/src/plan/chunk_parent.rs  parent_hashes (the read rule: cut the text)
\*   crates/chunk-split/src/list.rs          chunk_list (the list a RED reader trusts)
\*   crates/crdt/src/field/lww.rs            merge (the per-field tie break)
\*   crates/db/src/chunk/guard.rs            check_chunk_merge
\*   crates/db/src/merge/merge_handler/composite_persist.rs  the guard's call on a chunk merge
\*   crates/db/src/chunk/peer_delete.rs      delete_must_wait (a chunk delete waits for its article)
\*   crates/db/src/merge/merge_handler/doc_identity.rs  resolve_composite_doc_id_in_txn (a delete needs its chunk's first block)
\*   crates/db/src/merge/merge_handler/composite.rs     process_composite_delta_locked (older blocks merge first)
\* The deleted-parent guard, the read-time hash check and races with a reader are
\* ChunkRead.tla's. An encrypted chunk's hash can't be checked at merge, so that
\* case is not modelled here.
\* Numbers and finite sets are all this model needs.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Peers,       \* the two peers, for example {p1, p2}
  MaxEdits,    \* how many edits each peer makes, at most
  CutText,     \* TRUE: a reader cuts the current text and ignores the list (the fix)
  CheckHash,   \* TRUE: the merge refuses a chunk whose hash does not match its text (the fix)
  Immutable,   \* TRUE: the merge refuses any change to a chunk that is already stored (the fix)
  Deletes,     \* TRUE: the article and its chunks can be deleted (kept off where a model would be too big)
  DeleteRule,  \* how a chunk delete is judged: "wait" (the fix), "listed", "none" or "refuse"
  GenesisFirst \* TRUE: a chunk delete merges only after the chunk's first block (the fix)

\* Each peer edits at least once.
ASSUME MaxEdits \in Nat \ {0}
\* The switches are yes-or-no.
ASSUME CutText \in BOOLEAN
ASSUME CheckHash \in BOOLEAN
ASSUME Immutable \in BOOLEAN
\* Deletes are on or off.
ASSUME Deletes \in BOOLEAN
\* "listed" waits only for a chunk the text uses now, "none" merges every chunk
\* delete, and "refuse" refuses a blocked one for good.
ASSUME DeleteRule \in {"wait", "listed", "none", "refuse"}
\* Needing the chunk's first block is on or off.
ASSUME GenesisFirst \in BOOLEAN

\* The texts an edit may write.
Texts == {"t1", "t2", "t3"}

\* The chunks the splitter cuts each text into.
ChunksOf(t) == CASE t = "t1" -> {"a", "b"}
                 [] t = "t2" -> {"a", "c"}
                 [] t = "t3" -> {"d"}

\* How two texts compare in a tie: the bytes of the text.
TextRank(t) == CASE t = "t1" -> 1 [] t = "t2" -> 2 [] t = "t3" -> 3

\* How two lists compare in a tie: the bytes of the list, which start with a
\* hash, so they order the texts differently.
ListRank(t) == CASE t = "t1" -> 3 [] t = "t2" -> 1 [] t = "t3" -> 2

\* Every chunk any text is cut into.
AllChunks == UNION {ChunksOf(t) : t \in Texts}

\* An edit: who made it, at which height, and the text it wrote.
Edits == [peer : Peers, height : 1..(2 * MaxEdits), text : Texts]

VARIABLES
  made,      \* every edit made so far, by anyone
  known,     \* known[p]: the edits peer p has merged
  chunks,    \* chunks[p]: the chunks stored on peer p
  count,     \* count[p]: how many edits peer p has made
  forged,    \* forged[p]: chunks stored on p whose hash does not match their text
  changed,   \* changed[p]: chunks on p that were changed after they were stored
  dead,      \* dead[p]: whether peer p holds the article's delete
  chunkDels, \* the chunks some article delete deleted, whose deletes now travel
  gone,      \* gone[p]: the chunks deleted on peer p
  refused    \* refused[p]: the chunk deletes peer p refused for good

\* All the variables, for stuttering steps.
vars == <<made, known, chunks, count, forged, changed, dead, chunkDels, gone, refused>>

\* The edit among `es` that wins under a tie break `rank`: highest height
\* first, then the higher rank.
Winner(es, rank(_)) ==
  CHOOSE e \in es : \A f \in es :                         \* e beats every other edit f:
    \/ f.height < e.height                                 \* f is lower,
    \/ (f.height = e.height /\ rank(f.text) <= rank(e.text)) \* or as high and not bigger

\* The text peer p holds: the winner by height, then text bytes.
TextOn(p) == Winner(known[p], TextRank).text

\* The text peer p's list was cut from: the winner by height, then list bytes.
ListTextOn(p) == Winner(known[p], ListRank).text

\* The chunks the read rule allows on peer p.
Allowed(p) ==
  IF CutText
  \* The fix: cut the text peer p holds.
  THEN ChunksOf(TextOn(p))
  \* The bug: trust the list, which may come from another edit.
  ELSE ChunksOf(ListTextOn(p))

\* The chunks a reader on peer p sees: allowed, stored there and not deleted.
\* A peer that has merged nothing, or the article's delete, has nothing to show.
Shown(p) == IF known[p] = {} \/ dead[p] THEN {} ELSE (Allowed(p) \cap chunks[p]) \ gone[p]

\* Whether a chunk delete of c must not merge on p yet: the article is not gone
\* here. The "listed" rule blocks only a chunk the article uses now.
Blocked(p, c) ==
  /\ ~dead[p]                                                     \* the article is not gone here
  /\ (DeleteRule = "listed" => (known[p] = {} \/ c \in Allowed(p)))  \* and, for "listed", uses c

\* The heights an edit may use: one above what the peer has merged.
NextHeight(p) ==
  IF known[p] = {} THEN 1                       \* a first edit is at height 1
  ELSE 1 + Winner(known[p], TextRank).height    \* later ones one above the top

\* The type of every variable.
TypeOK ==
  /\ made \subseteq Edits                                          \* edits are edits
  /\ known \in [Peers -> SUBSET Edits]                              \* each peer knows some
  /\ chunks \in [Peers -> SUBSET AllChunks]                         \* stored chunks are real chunks
  /\ count \in [Peers -> 0..MaxEdits]                               \* edits made, per peer
  /\ forged \in [Peers -> SUBSET AllChunks]                         \* forged ones are real chunk names
  /\ changed \in [Peers -> SUBSET AllChunks]                        \* changed ones are real chunk names
  /\ dead \in [Peers -> BOOLEAN]                                     \* each peer holds the delete or not
  /\ chunkDels \subseteq AllChunks                                   \* deleted chunks are real chunk names
  /\ gone \in [Peers -> SUBSET AllChunks]                           \* so are the ones gone on a peer
  /\ refused \in [Peers -> SUBSET AllChunks]                        \* and the refused ones

\* At the start no one has edited anything.
Init ==
  /\ made = {}                        \* no edits yet
  /\ known = [p \in Peers |-> {}]      \* no peer knows any
  /\ chunks = [p \in Peers |-> {}]     \* no chunks stored
  /\ count = [p \in Peers |-> 0]       \* no peer has edited
  /\ forged = [p \in Peers |-> {}]     \* nothing forged is stored
  /\ changed = [p \in Peers |-> {}]    \* nothing was changed
  /\ dead = [p \in Peers |-> FALSE]     \* the article is not deleted
  /\ chunkDels = {}                     \* no chunk delete exists
  /\ gone = [p \in Peers |-> {}]       \* no chunk is deleted
  /\ refused = [p \in Peers |-> {}]    \* nothing was refused

\* Peer p edits the article to text t: one write sets text and list, and the
\* chunks of t are created on p in the same transaction.
Edit(p, t) ==
  \* p still has edits left.
  /\ count[p] < MaxEdits
  \* The new edit sits one above what p has seen.
  /\ LET e == [peer |-> p, height |-> NextHeight(p), text |-> t]
     IN /\ made' = made \cup {e}                            \* the edit exists
        /\ known' = [known EXCEPT ![p] = @ \cup {e}]         \* and p has it
  /\ chunks' = [chunks EXCEPT ![p] = @ \cup ChunksOf(t)]    \* p stores t's chunks
  /\ count' = [count EXCEPT ![p] = @ + 1]                  \* one more edit by p
  /\ ~dead[p]                                              \* p's article is not deleted
  /\ UNCHANGED <<forged, changed, dead, chunkDels, gone, refused>>  \* no lie or delete is involved

\* Peer p merges an edit another peer made. Both fields merge from it.
MergeEdit(p) ==
  \E e \in made \ known[p] :                      \* an edit p lacks
    /\ known' = [known EXCEPT ![p] = @ \cup {e}]   \* is merged on p
    /\ UNCHANGED <<made, chunks, count, forged, changed, dead, chunkDels, gone, refused>>  \* nothing else changes

\* A chunk document reaches peer p from the peer that created it.
MergeChunk(p) ==
  \E q \in Peers \ {p} :              \* another peer q
    \E c \in chunks[q] \ chunks[p] :   \* stores a chunk p lacks
      \* The merge guard accepts it: its hash is its text's, and it never changes.
      /\ chunks' = [chunks EXCEPT ![p] = @ \cup {c}]  \* p now stores it
      /\ UNCHANGED <<made, known, count, forged, changed, dead, chunkDels, gone, refused>>  \* nothing else changes

\* A peer sends p a chunk whose hash does not match its text.
PlantForged(p) ==
  /\ ~CheckHash                                         \* only when the merge does not check the hash
  /\ \E c \in AllChunks :                               \* some chunk name
       /\ chunks' = [chunks EXCEPT ![p] = @ \cup {c}]   \* p stores it
       /\ forged' = [forged EXCEPT ![p] = @ \cup {c}]   \* and it is a forgery
  /\ UNCHANGED <<made, known, count, changed, dead, chunkDels, gone, refused>>  \* nothing else changes

\* A peer sends p a new version of a chunk p already stores, with a hash that
\* matches the new version. The hash check alone can't tell.
Rewrite(p) ==
  /\ ~Immutable                                         \* only when the merge allows changes
  /\ \E c \in chunks[p] :                               \* a chunk p stores
       changed' = [changed EXCEPT ![p] = @ \cup {c}]    \* is now a different chunk
  /\ UNCHANGED <<made, known, count, chunks, forged, dead, chunkDels, gone, refused>>  \* nothing else changes

\* Peer p deletes the article it holds, and every chunk it stores with it.
DeleteArticle(p) ==
  /\ Deletes                                              \* deletes are modelled
  /\ known[p] # {} /\ ~dead[p]                            \* p holds a live article
  /\ dead' = [dead EXCEPT ![p] = TRUE]                     \* p deletes it
  /\ chunkDels' = chunkDels \cup chunks[p]                 \* its chunk deletes now travel
  /\ gone' = [gone EXCEPT ![p] = @ \cup chunks[p]]         \* and its chunks are gone on p
  /\ UNCHANGED <<made, known, chunks, count, forged, changed, refused>>  \* nothing else changes

\* The article's delete reaches peer p from a peer that holds it.
MergeArticleDelete(p) ==
  /\ ~dead[p] /\ \E q \in Peers : dead[q]                  \* p lacks a delete some peer holds
  /\ dead' = [dead EXCEPT ![p] = TRUE]                     \* p holds it now
  /\ UNCHANGED <<made, known, chunks, count, forged, changed, chunkDels, gone, refused>>  \* nothing else changes

\* A delete of chunk c reaches peer p: an honest one an article delete made,
\* or a forged one. DeleteRule decides what p does with it.
ReceiveChunkDelete(p) ==
  /\ Deletes                                              \* deletes are modelled
  /\ \E c \in chunks[p] \ (gone[p] \cup refused[p]) :       \* a chunk p stores and still judges
      \* With "wait" or "listed" a blocked delete does not merge yet; it stays on its way.
      /\ (DeleteRule \in {"wait", "listed"}) => ~Blocked(p, c)
      \* With "refuse" a blocked delete is refused for good.
      /\ IF DeleteRule = "refuse" /\ Blocked(p, c)
         THEN /\ refused' = [refused EXCEPT ![p] = @ \cup {c}]  \* refused
              /\ UNCHANGED gone                                 \* and the chunk stays
         ELSE /\ gone' = [gone EXCEPT ![p] = @ \cup {c}]        \* the delete merges
              /\ UNCHANGED refused                              \* nothing refused
      /\ UNCHANGED <<made, known, chunks, count, forged, changed, dead, chunkDels>>  \* nothing else changes

\* A delete of chunk c reaches peer p before c does. With the fix it can't name
\* c yet, so it does not merge; the bug merges it with no article check.
ReceiveUnstoredChunkDelete(p) ==
  /\ Deletes                                                   \* deletes are modelled
  /\ ~GenesisFirst                                             \* only when a delete can merge before its chunk
  /\ \E c \in AllChunks \ (chunks[p] \cup gone[p] \cup refused[p]) : \* a chunk p does not store
       gone' = [gone EXCEPT ![p] = @ \cup {c}]                  \* is marked deleted on p
  /\ UNCHANGED <<made, known, chunks, count, forged, changed, dead, chunkDels, refused>>  \* nothing else changes

\* Every step a peer can take.
Next ==
  \E p \in Peers :
    \/ \E t \in Texts : Edit(p, t)  \* p edits the article,
    \/ MergeEdit(p)                 \* merges an edit,
    \/ MergeChunk(p)                \* receives a chunk,
    \/ PlantForged(p)               \* is sent a forged chunk,
    \/ Rewrite(p)                   \* is sent a change to a stored chunk,
    \/ DeleteArticle(p)             \* deletes the article,
    \/ MergeArticleDelete(p)        \* merges the article's delete,
    \/ ReceiveChunkDelete(p)        \* is sent a chunk delete,
    \/ ReceiveUnstoredChunkDelete(p) \* or a chunk delete before the chunk

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* A reader shows only chunks of the text it shows.
INV_ReadsMatchText ==
  \A p \in Peers : known[p] # {} => Shown(p) \subseteq ChunksOf(TextOn(p))  \* per peer holding the article

\* No peer stores a chunk whose hash does not match its text.
INV_NoForgedChunk == \A p \in Peers : forged[p] = {}  \* nothing forged anywhere

\* No stored chunk is ever changed.
INV_ChunksNeverChange == \A p \in Peers : changed[p] = {}  \* nothing changed anywhere

\* A live article keeps every stored chunk its text uses.
INV_ListedChunksStay ==
  \A p \in Peers : (known[p] # {} /\ ~dead[p]) => (Allowed(p) \cap chunks[p] \cap gone[p]) = {}  \* per live article

\* A peer that holds the article's delete never refused one of its chunk deletes.
INV_DeletesNotLost == \A p \in Peers : dead[p] => (refused[p] \cap chunkDels) = {}  \* per deleted article

\* Every edit and chunk has reached every peer.
Settled ==
  /\ made # {}                                    \* someone edited
  /\ \A p \in Peers : known[p] = made              \* every peer has every edit
  /\ \A p, q \in Peers : chunks[p] = chunks[q]    \* and every chunk
  /\ \A p \in Peers : ~dead[p]                     \* and the article is live

\* Settled peers show the same chunks: all of the text's, and no others.
INV_SettledPeersAgree ==
  Settled =>
    \A p, q \in Peers :                       \* for any two peers,
      /\ Shown(p) = Shown(q)                  \* they show the same chunks,
      /\ Shown(p) = ChunksOf(TextOn(p))       \* exactly those of the text
====

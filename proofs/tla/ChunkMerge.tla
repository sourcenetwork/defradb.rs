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
\* A reader shows the chunks of the current text. Entry 0 of the list is the
\* hash of the text the list was cut from; when it is not the hash of the
\* current text, the reader cuts the current text itself instead of trusting
\* the list.
\*
\* What must never happen:
\*   - a reader shows a chunk that is not part of the current text (INV_ReadsMatchText);
\*   - once every edit and chunk has reached both peers, they show different
\*     chunks, or miss a chunk of the text (INV_SettledPeersAgree).
\*
\* Code it models:
\*   crates/query/src/plan/chunk_parent.rs  parent_hashes (the read rule)
\*   crates/chunk-split/src/list.rs          describes, chunk_list (entry 0)
\*   crates/crdt/src/lww.rs                  the per-field tie break
\*   crates/db/src/chunk/sync/mod.rs         set_lists (text and list in one write)
\*   crates/db/src/chunk/guard.rs            check_chunk_merge
\* The deleted-parent and hash guards, and races with a reader, are ChunkRead.tla's.
\* Numbers and finite sets are all this model needs.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Peers,       \* the two peers, for example {p1, p2}
  MaxEdits,    \* how many edits each peer makes, at most
  CheckEntry0  \* TRUE: a reader checks entry 0 against the text (the fix)

\* Each peer edits at least once.
ASSUME MaxEdits \in Nat \ {0}
\* The switch is yes-or-no.
ASSUME CheckEntry0 \in BOOLEAN

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

\* An edit: who made it, at which height, and the text it wrote.
Edits == [peer : Peers, height : 1..(2 * MaxEdits), text : Texts]

VARIABLES
  made,      \* every edit made so far, by anyone
  known,     \* known[p]: the edits peer p has merged
  chunks,    \* chunks[p]: the chunks stored on peer p
  count      \* count[p]: how many edits peer p has made

\* All the variables, for stuttering steps.
vars == <<made, known, chunks, count>>

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
  \* With the fix, a list cut from other text is not trusted.
  IF CheckEntry0 /\ ListTextOn(p) # TextOn(p)
  \* Entry 0 names other text: cut the current text instead.
  THEN ChunksOf(TextOn(p))
  \* Otherwise trust the list.
  ELSE ChunksOf(ListTextOn(p))

\* The chunks a reader on peer p sees: allowed, and stored there.
\* A peer that has merged nothing has no article to show.
Shown(p) == IF known[p] = {} THEN {} ELSE Allowed(p) \cap chunks[p]

\* The heights an edit may use: one above what the peer has merged.
NextHeight(p) ==
  IF known[p] = {} THEN 1                       \* a first edit is at height 1
  ELSE 1 + Winner(known[p], TextRank).height    \* later ones one above the top

\* The type of every variable.
TypeOK ==
  /\ made \subseteq Edits                                          \* edits are edits
  /\ known \in [Peers -> SUBSET Edits]                              \* each peer knows some
  /\ chunks \in [Peers -> SUBSET UNION {ChunksOf(t) : t \in Texts}] \* stored chunks are real chunks
  /\ count \in [Peers -> 0..MaxEdits]                               \* edits made, per peer

\* At the start no one has edited anything.
Init ==
  /\ made = {}                        \* no edits yet
  /\ known = [p \in Peers |-> {}]      \* no peer knows any
  /\ chunks = [p \in Peers |-> {}]     \* no chunks stored
  /\ count = [p \in Peers |-> 0]       \* no peer has edited

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

\* Peer p merges an edit another peer made. Both fields merge from it.
MergeEdit(p) ==
  \E e \in made \ known[p] :                      \* an edit p lacks
    /\ known' = [known EXCEPT ![p] = @ \cup {e}]   \* is merged on p
    /\ UNCHANGED <<made, chunks, count>>          \* nothing else changes

\* A chunk document reaches peer p from the peer that created it.
MergeChunk(p) ==
  \E q \in Peers \ {p} :              \* another peer q
    \E c \in chunks[q] \ chunks[p] :   \* stores a chunk p lacks
      \* The merge guard accepts it: its hash is its text's, and it never changes.
      /\ chunks' = [chunks EXCEPT ![p] = @ \cup {c}]  \* p now stores it
      /\ UNCHANGED <<made, known, count>>            \* nothing else changes

\* Every step a peer can take.
Next ==
  \E p \in Peers :
    \/ \E t \in Texts : Edit(p, t)  \* p edits the article,
    \/ MergeEdit(p)                 \* merges an edit,
    \/ MergeChunk(p)                 \* or receives a chunk

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* A reader shows only chunks of the text it shows.
INV_ReadsMatchText ==
  \A p \in Peers : known[p] # {} => Shown(p) \subseteq ChunksOf(TextOn(p))  \* per peer holding the article

\* Every edit and chunk has reached every peer.
Settled ==
  /\ made # {}                                    \* someone edited
  /\ \A p \in Peers : known[p] = made              \* every peer has every edit
  /\ \A p, q \in Peers : chunks[p] = chunks[q]    \* and every chunk

\* Settled peers show the same chunks: all of the text's, and no others.
INV_SettledPeersAgree ==
  Settled =>
    \A p, q \in Peers :                       \* for any two peers,
      /\ Shown(p) = Shown(q)                  \* they show the same chunks,
      /\ Shown(p) = ChunksOf(TextOn(p))       \* exactly those of the text
====

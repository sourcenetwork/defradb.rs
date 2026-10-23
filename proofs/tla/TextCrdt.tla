---- MODULE TextCrdt ----
\* The story, in plain words.
\*
\* A Text field holds a string that many peers edit at once, offline or not.
\* Every edit is one block in the document's DAG. A block names its parents,
\* the heads its author had when it wrote the edit, and says one thing:
\* "insert a character at position p" or "delete the character at position
\* p". The position counts characters in the block's parent version: the text
\* that its parents, and everything before them, produce. It does not count
\* characters in whatever text a receiving peer happens to hold.
\*
\* Peers swap blocks in any order the network allows. Each peer turns the
\* blocks it holds into text with a Fugue tree. Every character is a node.
\* A new character goes right after the character at position p-1 of its
\* parent version (the "left origin"): it becomes the left origin's right
\* child if the left origin has none, and otherwise the left child of the
\* node that comes right after the left origin in the tree's order. Two
\* characters that land on the same spot (siblings) are ordered by their
\* block's ID, the CID. Reading the tree left to right, skipping deleted
\* characters, gives the text. A delete only marks its character deleted, so
\* concurrent edits that point at it still find it.
\*
\* Example: the text is "a". Alice and Bob each insert a character at
\* position 1, right after a, at the same time: Alice "x", Bob "y". Each
\* block says "position 1 of a". In that version a has no right child, so
\* both characters become right children of a, siblings ordered by ID: with
\* x's ID smaller, both peers show "axy". If Bob's peer instead read Alice's
\* position against its own "ay", a would already have a right child, x
\* would go before y, and Alice's peer, doing the same with Bob's block,
\* would put y before x: "axy" on one peer, "ayx" on the other.
\*
\* What must never happen:
\*   * two peers holding the same blocks show different text;
\*   * a peer's text differs from replaying its blocks from scratch;
\*   * an insert lands anywhere but right after its left origin in its own
\*     parent version, or a delete removes anything but its own character;
\*   * a peer places a character somewhere other than where its author's
\*     version put it.
\*
\* Code this models (defradb.rs). The Text CRDT is not written yet: this model
\* is its contract (issue #2127), and these are the files it will live in:
\*   crates/crdt/src/text.rs                       the Text CRDT: Fugue
\*                                                 placement, sibling order by
\*                                                 CID, tombstones, replay
\*   crates/db/src/merge/merge_handler/text.rs     applies a received Text block
\*                                                 once its parents are merged
\*
\* Correspondence table (model -> planned Rust):
\*   Create         text.rs: a write diffs the new string against the current
\*                  one and emits one block per edit, its parents the heads
\*   Deliver        merge_handler/text.rs: a received block is merged once
\*                  every parent block is local (the DAG sync's causal buffer)
\*   Place          text.rs: Fugue placement against the block's parent version
\*   SortBy/IdKey   text.rs: siblings ordered by the block CID
\*   Canon          text.rs: replay of a block set from scratch; the stored
\*                  value is the replayed text, the tree is thrown away
\*   blocks         the document's DAG blocks for one Text field
\*   applied        the blocks a peer has merged
\*   tree, dead     the replay state a peer holds while it merges
\*   order          the order a peer merged blocks in, used only by the RED
\*                  configuration that orders siblings by arrival

EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Peers,        \* the peers editing one Text field
  MaxBlocks,    \* how many edit blocks may be written in all, so the model stays finite
  SiblingKey,   \* "id": siblings ordered by block ID (the design); "arrival": by arrival order
  CausalBuffer, \* TRUE: a block is merged only after its parents (the design)
  ResolveAt     \* "parent": positions read in the block's parent version (the design);
                \* "current": in the receiving peer's current text

VARIABLES
  blocks,   \* the blocks written so far, in the order they were written; the index is the block ID
  applied,  \* for each peer, the IDs of the blocks it has merged
  tree,     \* for each peer, its Fugue tree: one [id, parent, side] node per inserted character
  dead,     \* for each peer, its deletions: one [by, target] pair per merged delete block
  order     \* for each peer, the block IDs in the order it merged them

\* Every variable, for "nothing changed".
vars == <<blocks, applied, tree, dead, order>>

\* The IDs blocks may take. A real CID is a hash; here it is a number, and IDs
\* are handed out in the order blocks are written. Since the model tries every
\* order of writes, two concurrent blocks get their IDs both ways round.
Ids == 1..MaxBlocks

\* The tree's root: a node that is not a character and is never shown.
Root == 0

\* The smaller of two numbers.
Min(a, b) == IF a < b THEN a ELSE b

\* The IDs of the blocks written so far.
Created == 1..Len(blocks)

\* A set of blocks together with every block before them in the DAG.
Closure(S) == S \cup UNION {blocks[b].anc : b \in S}

\* The place in a sequence where x sits (x is in it exactly once).
IndexOf(s, x) == CHOOSE i \in 1..Len(s) : s[i] = x

\* The elements of S as a sequence, smallest key first. k gives every element a distinct key.
RECURSIVE SortBy(_, _)
SortBy(S, k) ==
  IF S = {} THEN <<>>                                   \* nothing left to place
  ELSE LET m == CHOOSE x \in S : \A y \in S : k[x] <= k[y] \* the element with the smallest key
       IN <<m>> \o SortBy(S \ {m}, k)                    \* it first, then the rest in order

\* The children of node n on one side ("L" or "R") of tree T.
Kids(T, n, s) == {x.id : x \in {y \in T : y.parent = n /\ y.side = s}}

\* Reading tree T from node n, left to right: n's left children (each with
\* everything under it), then n itself, then its right children. Siblings go
\* in key order. The root itself is not a character, so it is left out.
RECURSIVE Trav(_, _, _), TravAll(_, _, _)
Trav(T, n, k) ==
  TravAll(T, SortBy(Kids(T, n, "L"), k), k)  \* everything left of n
  \o (IF n = Root THEN <<>> ELSE <<n>>)      \* n itself, unless it is the root
  \o TravAll(T, SortBy(Kids(T, n, "R"), k), k) \* everything right of n

\* Reading each node of the sequence s in turn, with everything under it.
TravAll(T, s, k) ==
  IF s = <<>> THEN <<>>                          \* no node left
  ELSE Trav(T, Head(s), k) \o TravAll(T, Tail(s), k) \* the first one, then the rest

\* The text a tree shows: every character in order, skipping the deleted ones in D.
Visible(T, D, k) == SelectSeq(Trav(T, Root, k), LAMBDA x : x \notin D)

\* Where Fugue puts a character inserted at position pos of the text T and D
\* show: its parent node and side.
Place(T, D, pos, k) ==
  LET full == Trav(T, Root, k)                          \* every character, deleted ones too
      vis  == Visible(T, D, k)                          \* the characters shown
      p    == Min(pos, Len(vis))                        \* a position past the end means the end
      lo   == IF p = 0 THEN Root ELSE vis[p]            \* the left origin: the character before
      i    == IF lo = Root THEN 0 ELSE IndexOf(full, lo) \* where the left origin sits, all counted
  IN IF Kids(T, lo, "R") = {}
       THEN [parent |-> lo, side |-> "R"]               \* no right child: become it
       ELSE [parent |-> full[i + 1], side |-> "L"]      \* else the left child of the next node

\* The character a delete at position pos of the text T and D show removes.
\* An empty text has nothing to remove, which the root stands for.
Target(T, D, pos, k) ==
  LET vis == Visible(T, D, k)                        \* the characters shown
  IN IF vis = <<>> THEN Root                          \* nothing to remove
     ELSE vis[Min(pos + 1, Len(vis))]                 \* the character at that position

\* The part of tree T written by the blocks in V.
Restrict(T, V) == {x \in T : x.id \in V}

\* The characters the deletions in Dl that belong to blocks in V remove.
DeadIn(Dl, V) == {d.target : d \in {x \in Dl : x.by \in V}}

\* Siblings ordered by block ID: the key of an ID is the ID itself.
IdKey == [i \in 0..MaxBlocks |-> i]

\* One block b merged into the replay state st (a tree and its deletions),
\* with positions read in the part of st written by the blocks in V, and
\* siblings ordered by k.
Step(st, b, blk, V, k) ==
  LET T == Restrict(st.tree, V)    \* the tree as the version V shows it
      D == DeadIn(st.dead, V)      \* the deletions the version V shows
  IN IF blk.kind = "ins"
       THEN [st EXCEPT !.tree = @ \cup                         \* add one node:
              {[id |-> b,                                       \* the new character,
                parent |-> Place(T, D, blk.pos, k).parent,      \* under the node Fugue picks,
                side |-> Place(T, D, blk.pos, k).side]}]        \* on the side Fugue picks
       ELSE [st EXCEPT !.dead = @ \cup                         \* add one deletion:
              {[by |-> b, target |-> Target(T, D, blk.pos, k)]}] \* this block removes its character

\* Replaying the blocks in S from scratch, smallest ID first. IDs follow the
\* order blocks were written, so every block comes after its parents, and each
\* reads its position in exactly its own parent version.
RECURSIVE CanonFrom(_, _)
CanonFrom(S, st) ==
  IF S = {} THEN st                                          \* every block replayed
  ELSE LET b == CHOOSE x \in S : \A y \in S : x <= y          \* the oldest block left
       IN CanonFrom(S \ {b}, Step(st, b, blocks[b], blocks[b].anc, IdKey)) \* merge it, go on

\* The replay of S from an empty state.
Canon(S) == CanonFrom(S, [tree |-> {}, dead |-> {}])

\* The text replaying S from scratch shows.
CanonText(S) ==
  LET st == Canon(S)                                          \* the replayed state
  IN Visible(st.tree, {d.target : d \in st.dead}, IdKey)      \* read with siblings by ID

\* How peer p orders siblings, given the order ord it merged blocks in.
PeerKey(ord) ==
  [i \in 0..MaxBlocks |->
     IF SiblingKey = "id" THEN i                              \* the design: by block ID
     ELSE IF \E j \in 1..Len(ord) : ord[j] = i                \* the bug: by when it arrived
          THEN IndexOf(ord, i)
          ELSE 0]                                              \* the root, or not yet arrived

\* The text peer p shows.
PeerText(p) == Visible(tree[p], {d.target : d \in dead[p]}, PeerKey(order[p]))

\* The replay state peer p holds after merging block b, with record blk.
\* The design reads positions in the block's parent version; the RED
\* configuration reads them in everything the peer already holds.
Merged(p, b, blk) ==
  Step([tree |-> tree[p], dead |-> dead[p]],                 \* the peer's state now
       b, blk,                                               \* the block to merge
       IF ResolveAt = "parent" THEN blk.anc ELSE applied[p], \* the version positions count in
       PeerKey(order[p]))                                    \* how it orders siblings

\* Peer p records block b, with record blk, as merged.
Record(p, b, blk) ==
  LET st == Merged(p, b, blk)                             \* the state after merging
  IN /\ tree' = [tree EXCEPT ![p] = st.tree]              \* its new tree
     /\ dead' = [dead EXCEPT ![p] = st.dead]              \* its new deletions
     /\ applied' = [applied EXCEPT ![p] = @ \cup {b}]     \* b is merged here
     /\ order' = [order EXCEPT ![p] = Append(@, b)]       \* and arrived last

\* The kinds of value each variable may hold.
TypeOK ==
  /\ Len(blocks) <= MaxBlocks                                \* no more blocks than allowed
  /\ \A i \in 1..Len(blocks) :                               \* each block is
       blocks[i] \in [kind : {"ins", "del"},                 \* an insert or a delete,
                      pos : 0..MaxBlocks,                    \* at a position,
                      anc : SUBSET Ids]                      \* with the blocks before it
  /\ applied \in [Peers -> SUBSET Ids]                       \* each peer merged some blocks
  /\ order \in [Peers -> Seq(Ids)]                           \* in some order
  /\ \A p \in Peers : \A x \in tree[p] : x.id \in Ids        \* tree nodes are characters
  /\ \A p \in Peers : \A d \in dead[p] : d.by \in Ids        \* deletions come from blocks

\* At the start no block exists and every text is empty.
Init ==
  /\ blocks = <<>>                      \* nothing written
  /\ applied = [p \in Peers |-> {}]     \* nothing merged
  /\ tree = [p \in Peers |-> {}]        \* every tree is just the root
  /\ dead = [p \in Peers |-> {}]        \* nothing deleted
  /\ order = [p \in Peers |-> <<>>]     \* nothing arrived

\* Peer q writes an edit of kind kind at position pos of the text it shows.
\* Its parents are the blocks it has merged, and it merges its own block at once.
Create(q, kind, pos) ==
  /\ Len(blocks) < MaxBlocks                       \* still room for a block
  /\ LET b   == Len(blocks) + 1                    \* the new block's ID
         blk == [kind |-> kind, pos |-> pos,       \* what it says,
                 anc |-> Closure(applied[q])]      \* and the blocks before it
     IN /\ blocks' = Append(blocks, blk)           \* the block exists now
        /\ Record(q, b, blk)                       \* and its author has merged it

\* Some peer writes some edit: an insert anywhere in its text, or a delete of
\* one of its characters.
Write ==
  \E q \in Peers :                                      \* some peer
    LET n == Len(PeerText(q))                           \* the length of its text
    IN \/ \E pos \in 0..n : Create(q, "ins", pos)       \* inserts before, between or after
       \/ n > 0 /\ \E pos \in 0..(n - 1) : Create(q, "del", pos) \* or deletes a character

\* Block b reaches peer p and is merged. The design waits until every block
\* before it is merged; the RED configuration merges it on arrival.
Deliver(p, b) ==
  /\ b \in Created                                     \* the block exists
  /\ b \notin applied[p]                               \* the peer does not have it yet
  /\ CausalBuffer => blocks[b].anc \subseteq applied[p] \* its parents are merged here
  /\ Record(p, b, blocks[b])                           \* merge it
  /\ UNCHANGED blocks                                  \* no block is written

\* Every block is written and every peer has merged all of them.
Done == Len(blocks) = MaxBlocks /\ \A p \in Peers : applied[p] = Created

\* One step: a peer writes an edit, or a block reaches a peer. Once all is
\* written and merged nothing more happens, which is not a deadlock.
Next ==
  \/ Write                                      \* someone edits
  \/ \E p \in Peers, b \in Ids : Deliver(p, b)  \* or a block arrives
  \/ Done /\ UNCHANGED vars                     \* or everything has settled

\* The whole behaviour: start, then any sequence of steps.
Spec == Init /\ [][Next]_vars

\* Two peers that merged the same blocks show the same text.
INV_Converge ==
  \A p, q \in Peers : applied[p] = applied[q] => PeerText(p) = PeerText(q)

\* A set of blocks that holds every block before each of its own.
Closed(S) == Closure(S) = S

\* A peer's text, built one block at a time in whatever order they came, is the
\* text replaying its blocks from scratch shows.
INV_MatchesReplay ==
  \A p \in Peers : Closed(applied[p]) => PeerText(p) = CanonText(applied[p])

\* Sequence s with x put in after its first pos elements.
InsertAt(s, pos, x) == SubSeq(s, 1, pos) \o <<x>> \o SubSeq(s, pos + 1, Len(s))

\* Sequence s with its element number pos + 1 taken out.
RemoveAt(s, pos) == SubSeq(s, 1, pos) \o SubSeq(s, pos + 2, Len(s))

\* An insert at position p of its parent version lands right there, and a
\* delete at position p removes exactly that character; nothing else moves.
INV_Intent ==
  \A b \in Created :
    LET before == CanonText(blocks[b].anc)            \* the parent version's text
        after  == CanonText(blocks[b].anc \cup {b})   \* that text with this block added
    IN IF blocks[b].kind = "ins"
         THEN after = InsertAt(before, blocks[b].pos, b) \* the character is at its position
         ELSE after = RemoveAt(before, blocks[b].pos)    \* the character at its position is gone

\* Every peer puts each character, and each deletion, exactly where the
\* block's author put it in the block's own parent version.
INV_PlacedAsIntended ==
  \A p \in Peers :
    /\ \A x \in tree[p] : x \in Canon(blocks[x.id].anc \cup {x.id}).tree \* same parent and side
    /\ \A d \in dead[p] : d \in Canon(blocks[d.by].anc \cup {d.by}).dead  \* same character removed
====

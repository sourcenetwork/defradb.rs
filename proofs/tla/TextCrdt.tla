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
\* A write of null is a block too: it clears every character of its parent
\* version, and the field reads null. A peer that merges blocks one at a time
\* keeps a null flag beside its text. The field is null exactly when every head
\* of its history is such a clear: anything written beside or after a clear
\* that is not one makes the field text again, even an empty one.
\*
\* What must never happen:
\*   * two peers holding the same blocks show different text;
\*   * two peers holding the same blocks disagree on whether the field is
\*     null, a peer's null flag differs from what its heads say, or a null
\*     field still shows characters;
\*   * a peer's text differs from replaying its blocks from scratch;
\*   * an insert lands anywhere but right after its left origin in its own
\*     parent version, or a delete removes anything but its own character;
\*   * a peer places a character somewhere other than where its author's
\*     version put it.
\*   * a character typed right after another lands apart from it: something
\*     written without knowing the first character sits between the two, so
\*     two runs typed at one spot at the same time interleave.
\*
\* Code this models (defradb.rs). A real block carries a list of edits, each
\* an insert of a run of characters or a delete of a range. The model's blocks
\* hold one character each; a real block is those one-character blocks in a
\* row, each the parent of the next, so every character of a run is the right
\* child of the one before it (crdt::fugue::Tree::apply).
\*
\* Merging a block that is concurrent with the field's heads does not replay
\* the whole history. It finds the last critical block: one that every other
\* block either comes before or after in the DAG. The text at that block is
\* known in length (every block records the length of its own version's text,
\* checked when it merged), and the stored text supplies its characters. The
\* replay starts from a chain of placeholder characters standing for that
\* text, and runs only the blocks after the critical one. The characters
\* deleted before it are gone from the chain, yet the result must equal a
\* full replay. A block that is merely an ancestor of every head is not
\* enough: something concurrent with it would be replayed against a text
\* that already holds it.
\*
\* Correspondence table (model -> Rust):
\*   Create         crates/db/src/write/autocommit/helpers.rs apply_field_ops:
\*                  a write diffs the stored text against the new value
\*                  (TextField's CausalCrdt::diff in
\*                  crates/crdt/src/field/text.rs) or takes the edits it was
\*                  given, and TextField::block_data writes them as one
\*                  block, its parents the field's heads; a write of null
\*                  keeps the null value and its block is the "nil" kind,
\*                  data CBOR null (TextField's NIL)
\*   Deliver        crates/db/src/merge/merge_handler/field_causal.rs
\*                  merge_field_causal: a received block is merged once every
\*                  parent block is local (the DAG sync's causal buffer); a
\*                  block whose parents are the field's heads is applied to
\*                  the stored text as plain edits (FieldCrdt::apply), which
\*                  INV_Intent makes equal to the replay; that block alone
\*                  decides whether the field is null (NullAfter)
\*   Place          crates/crdt/src/fugue.rs Tree::apply, the insert arm:
\*                  Fugue placement against the block's parent version
\*   Target         crates/crdt/src/fugue.rs Tree::apply, the delete arm
\*   SortBy/IdKey   crates/crdt/src/fugue.rs Tree::attach and Tree::cmp_id:
\*                  siblings ordered by block CID, then offset in the block
\*   Trav           crates/crdt/src/fugue.rs Tree::traverse
\*   Restrict,      crates/crdt/src/fugue.rs Tree::present and Tree::dead: a
\*   DeadIn         node or delete counts in a version when its block is in
\*                  the version's Ancestry
\*   Canon          crates/crdt/src/fugue.rs replay, through TextField's
\*                  CausalCrdt::replay, from causal_history in
\*                  merge_handler/field_causal.rs; the stored value is the
\*                  replayed text, the tree is thrown away
\*   PhReplay       crates/crdt/src/fugue.rs merge_text (Tree::with_placeholder
\*                  builds the chain), through TextField's CausalCrdt::merge_from,
\*                  called from merge_handler/field_causal.rs merge_field_causal
\*                  once critical_block has found the block; its length is
\*                  TextEdits::len of that block
\*   Critical       crates/db/src/merge/merge_handler/field_causal.rs
\*                  critical_block: the walk down from the heads and the new
\*                  block that stops when one block is left and no root was
\*                  passed; in debug builds, is_critical checks the block it
\*                  stops at the way Critical defines it: every other block
\*                  is before it or after it
\*   ReplayFrom     "critical" is critical_block; "ancestor" exists only for
\*                  the RED configuration
\*   PlaceRule      "fugue" is Tree::apply; "next" exists only for the RED
\*                  configuration that breaks non-interleaving
\*   NilTips        crates/crdt/src/field/text.rs nil_heads, through
\*                  text_value in TextField's CausalCrdt::replay and
\*                  merge_from: null when every head writes null
\*   NullRule       "heads" is NilTips on a concurrent merge; "last" exists
\*                  only for the RED configuration that takes the null flag
\*                  from whichever block arrived last
\*   blocks         the document's DAG blocks for one Text field
\*   applied        the blocks a peer has merged
\*   tree, dead     the replay state a peer holds while it merges
\*   isnull         whether the field reads null on a peer: the stored value
\*                  is null rather than a text
\*   order          the order a peer merged blocks in, used only by the RED
\*                  configuration that orders siblings by arrival
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Peers,        \* the peers editing one Text field
  MaxBlocks,    \* how many edit blocks may be written in all, so the model stays finite
  SiblingKey,   \* "id": siblings ordered by block ID (the design); "arrival": by arrival order
  CausalBuffer, \* TRUE: a block is merged only after its parents (the design)
  ResolveAt,    \* "parent": positions read in the block's parent version (the design);
                \* "current": in the receiving peer's current text
  PlaceRule,    \* "fugue": a character whose left origin has no right child becomes
                \* it (the design); "next": always the left child of the node after
                \* its left origin
  ReplayFrom,   \* "critical": a concurrent merge replays from a critical block (the
                \* design); "ancestor": from any block before every head
  NullRule      \* "heads": a concurrent merge reads null from the heads (the design);
                \* "last": from the block it merges, whatever came before

VARIABLES
  blocks,   \* the blocks written so far, in the order they were written; the index is the block ID
  applied,  \* for each peer, the IDs of the blocks it has merged
  tree,     \* for each peer, its Fugue tree: one [id, parent, side] node per inserted character
  dead,     \* for each peer, its deletions: one [by, target] pair per merged delete block
  order,    \* for each peer, the block IDs in the order it merged them
  isnull    \* for each peer, whether the field reads null there

\* Every variable, for "nothing changed".
vars == <<blocks, applied, tree, dead, order, isnull>>

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
  IN IF PlaceRule = "fugue" /\ Kids(T, lo, "R") = {}
       THEN [parent |-> lo, side |-> "R"]               \* no right child: become it
       ELSE IF i + 1 > Len(full)                         \* the RED rule, at the very end:
       THEN [parent |-> lo, side |-> "R"]               \* there is no next node to hang under
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
      vis == Visible(T, D, k)      \* the characters the version V shows
  IN IF blk.kind = "ins"
       THEN [st EXCEPT !.tree = @ \cup                         \* add one node:
              {[id |-> b,                                       \* the new character,
                parent |-> Place(T, D, blk.pos, k).parent,      \* under the node Fugue picks,
                side |-> Place(T, D, blk.pos, k).side]}]        \* on the side Fugue picks
       ELSE IF blk.kind = "del"
       THEN [st EXCEPT !.dead = @ \cup                         \* add one deletion:
              {[by |-> b, target |-> Target(T, D, blk.pos, k)]}] \* this block removes its character
       ELSE [st EXCEPT !.dead = @ \cup                         \* a write of null clears
              {[by |-> b, target |-> vis[i]] : i \in 1..Len(vis)}] \* every character the version shows

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

\* The blocks of S nothing in S comes after: the heads S leaves.
Tips(S) == {b \in S : \A c \in S : b \notin blocks[c].anc}

\* The blocks of S leave the field null: there are some, and every head writes null.
NilTips(S) == S # {} /\ \A t \in Tips(S) : blocks[t].kind = "nil"

\* Whether the field reads null on peer p once it merges block b, with record
\* blk. A block whose parents are the peer's heads (the blocks before it are
\* all the peer holds, as for a block its author writes) is the only head after
\* it, so it alone decides. Any other block merges beside a head, and the
\* design reads the new heads; the RED configuration still asks only the block.
NullAfter(p, b, blk) ==
  IF NullRule = "last" \/ blk.anc = Closure(applied[p]) \* the block lands on the peer's heads
    THEN blk.kind = "nil"                         \* null exactly when it writes null
    ELSE NilTips(applied[p] \cup {b})             \* else what the new heads say

\* Peer p records block b, with record blk, as merged.
Record(p, b, blk) ==
  LET st == Merged(p, b, blk)                             \* the state after merging
  IN /\ tree' = [tree EXCEPT ![p] = st.tree]              \* its new tree
     /\ dead' = [dead EXCEPT ![p] = st.dead]              \* its new deletions
     /\ isnull' = [isnull EXCEPT ![p] = NullAfter(p, b, blk)] \* whether it reads null
     /\ applied' = [applied EXCEPT ![p] = @ \cup {b}]     \* b is merged here
     /\ order' = [order EXCEPT ![p] = Append(@, b)]       \* and arrived last

\* The kinds of value each variable may hold.
TypeOK ==
  /\ Len(blocks) <= MaxBlocks                                \* no more blocks than allowed
  /\ \A i \in 1..Len(blocks) :                               \* each block is
       blocks[i] \in [kind : {"ins", "del", "nil"},          \* an insert, a delete or a null,
                      pos : 0..MaxBlocks,                    \* at a position,
                      anc : SUBSET Ids]                      \* with the blocks before it
  /\ applied \in [Peers -> SUBSET Ids]                       \* each peer merged some blocks
  /\ order \in [Peers -> Seq(Ids)]                           \* in some order
  /\ isnull \in [Peers -> BOOLEAN]                           \* each peer reads null or not
  /\ \A p \in Peers : \A x \in tree[p] : x.id \in Ids        \* tree nodes are characters
  /\ \A p \in Peers : \A d \in dead[p] : d.by \in Ids        \* deletions come from blocks

\* At the start no block exists and every text is empty.
Init ==
  /\ blocks = <<>>                      \* nothing written
  /\ applied = [p \in Peers |-> {}]     \* nothing merged
  /\ tree = [p \in Peers |-> {}]        \* every tree is just the root
  /\ dead = [p \in Peers |-> {}]        \* nothing deleted
  /\ order = [p \in Peers |-> <<>>]     \* nothing arrived
  /\ isnull = [p \in Peers |-> FALSE]  \* nothing reads null yet

\* Peer q writes an edit of kind kind at position pos of the text it shows.
\* Its parents are the blocks it has merged, and it merges its own block at once.
Create(q, kind, pos) ==
  /\ Len(blocks) < MaxBlocks                       \* still room for a block
  /\ LET b   == Len(blocks) + 1                    \* the new block's ID
         blk == [kind |-> kind, pos |-> pos,       \* what it says,
                 anc |-> Closure(applied[q])]      \* and the blocks before it
     IN /\ blocks' = Append(blocks, blk)           \* the block exists now
        /\ Record(q, b, blk)                       \* and its author has merged it

\* Some peer writes some edit: an insert anywhere in its text, a delete of
\* one of its characters, or a write of null.
Write ==
  \E q \in Peers :                                      \* some peer
    LET n == Len(PeerText(q))                           \* the length of its text
    IN \/ \E pos \in 0..n : Create(q, "ins", pos)       \* inserts before, between or after
       \/ n > 0 /\ \E pos \in 0..(n - 1) : Create(q, "del", pos) \* or deletes a character
       \/ Create(q, "nil", 0)                           \* or writes null

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

\* Two peers that merged the same blocks agree on whether the field is null.
INV_NullConverge ==
  \A p, q \in Peers : applied[p] = applied[q] => isnull[p] = isnull[q]

\* A peer's null flag, built one block at a time, is what its heads say.
INV_NullMatchesReplay ==
  \A p \in Peers : Closed(applied[p]) => (isnull[p] <=> NilTips(applied[p]))

\* A field that reads null shows no character.
INV_NullEmpty ==
  \A p \in Peers : isnull[p] => PeerText(p) = <<>>

\* A peer's text, built one block at a time in whatever order they came, is the
\* text replaying its blocks from scratch shows.
INV_MatchesReplay ==
  \A p \in Peers : Closed(applied[p]) => PeerText(p) = CanonText(applied[p])

\* Block x is critical in S: every other block of S comes before x or after it.
Critical(S, x) ==
  \A b \in S : b = x \/ b \in blocks[x].anc \/ x \in blocks[b].anc

\* The blocks a concurrent merge of S may replay from. The design takes the
\* critical ones; the RED configuration takes any block before every head.
StartPoints(S) ==
  IF ReplayFrom = "critical"
    THEN {x \in S : Critical(S, x)}                               \* every block on one side of x
    ELSE {x \in S : \A t \in Tips(S) : t = x \/ x \in blocks[t].anc} \* x is under every head

\* The text at x's version as a chain of placeholder characters: each the
\* right child of the one before, the first under the root. A placeholder
\* keeps the ID of the character it stands for; the deleted ones are gone.
Chain(base) ==
  {[id |-> base[i],                                  \* the character it stands for
    parent |-> IF i = 1 THEN Root ELSE base[i - 1],  \* hung on the one before it
    side |-> "R"] : i \in 1..Len(base)}              \* as its right child

\* The text replaying S from x shows: the placeholders for x's version, then
\* every block of S after x, each reading positions in its own parent version.
PhReplay(S, x) ==
  LET C  == {x} \cup blocks[x].anc                            \* x and everything before it
      st == CanonFrom(S \ C, [tree |-> Chain(CanonText(C)), dead |-> {}]) \* the rest replayed
  IN Visible(st.tree, {d.target : d \in st.dead}, IdKey)      \* read with siblings by ID

\* Replaying a peer's blocks from any start point the merge may pick gives the
\* same text as replaying them all.
INV_CriticalReplay ==
  \A p \in Peers : Closed(applied[p]) =>
    \A x \in StartPoints(applied[p]) : PhReplay(applied[p], x) = CanonText(applied[p])

\* Sequence s with x put in after its first pos elements.
InsertAt(s, pos, x) == SubSeq(s, 1, pos) \o <<x>> \o SubSeq(s, pos + 1, Len(s))

\* Sequence s with its element number pos + 1 taken out.
RemoveAt(s, pos) == SubSeq(s, 1, pos) \o SubSeq(s, pos + 2, Len(s))

\* An insert at position p of its parent version lands right there, a delete
\* at position p removes exactly that character, and a write of null removes
\* every character; nothing else moves.
INV_Intent ==
  \A b \in Created :
    LET before == CanonText(blocks[b].anc)            \* the parent version's text
        after  == CanonText(blocks[b].anc \cup {b})   \* that text with this block added
    IN IF blocks[b].kind = "ins"
         THEN after = InsertAt(before, blocks[b].pos, b) \* the character is at its position
       ELSE IF blocks[b].kind = "del"
         THEN after = RemoveAt(before, blocks[b].pos)    \* the character at its position is gone
         ELSE after = <<>>                               \* nothing is left

\* Every peer puts each character, and each deletion, exactly where the
\* block's author put it in the block's own parent version.
INV_PlacedAsIntended ==
  \A p \in Peers :
    /\ \A x \in tree[p] : x \in Canon(blocks[x.id].anc \cup {x.id}).tree \* same parent and side
    /\ \A d \in dead[p] : d \in Canon(blocks[d.by].anc \cup {d.by}).dead  \* same character removed

\* Block b2 was typed right after block b1: both insert, b1 is before b2 in the
\* DAG, and in the text b2's author made, b2 sits right after b1.
TypedAfter(b1, b2) ==
  /\ blocks[b1].kind = "ins"                               \* b1 inserts a character
  /\ blocks[b2].kind = "ins"                               \* so does b2
  /\ b1 \in blocks[b2].anc                                \* b2's author knew b1
  /\ LET t == CanonText(blocks[b2].anc \cup {b2})        \* the text right after b2
     IN \E i \in 1..(Len(t) - 1) : t[i] = b1 /\ t[i + 1] = b2 \* where b2 follows b1

\* A character typed right after another is never split from it by one
\* written without knowing the first: whatever sits between them in any
\* peer's text was written by someone who had seen b1. So two runs typed at
\* one spot at the same time each stay whole.
INV_NoInterleave ==
  \A p \in Peers : \A b1, b2 \in applied[p] :                \* on every peer, each pair
    TypedAfter(b1, b2) =>                                     \* of typed-after characters
      LET t == PeerText(p)                                    \* in the text it shows
      IN \A i, j, m \in 1..Len(t) :                           \* any character between them
           (t[i] = b1 /\ t[j] = b2 /\ i < m /\ m < j) => b1 \in blocks[t[m]].anc \* knew b1
====

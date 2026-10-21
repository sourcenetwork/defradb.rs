import TextCrdt.Fugue
/-!
# TextCrdt.Span: a run of characters is one node, and cutting it changes nothing

The story, in plain words. A concurrent merge replays from a critical block,
and the text at that block stands in as placeholders: a run of characters,
each the right child of the one before. The code keeps the whole run as one
node (a span) instead of one node per character. The run's left children hang
on its first character and its right children on its last, because in the
chain nothing else can hang anywhere else before an edit lands there.

When an edit lands inside the run, the code cuts the span in two: the first
part keeps the left children, and the second part becomes the first part's
only right child, taking over the old right children. Then the edit hangs on
whichever character it needs, which is now the edge of a piece.

What must never happen: cutting a span changes the tree, so the text, or
where a later edit lands, differs from the chain of single characters it
stands for. `split_run` promises the cut span is the very same tree, so every
Fugue law in `TextCrdt.Fugue` holds for it unchanged; `run_trav` promises a
span reads as its characters in order, between its left and right children.

Rust anchors, in `crates/crdt/src/text.rs`: `Node::len` is a run's length,
`Tree::with_placeholder` builds the one span, and `Tree::split` cuts it
(`split_run`); `Tree::end_at` and `Tree::start_at` cut where an edit lands;
`Order` counts a span's characters by its length (`run_trav`).
-/

namespace TextCrdt

-- A run of characters x then xs, each the right child of the one before; the
-- run's left children l hang on x, its right children r on its last character.
def runTree (x : Nat) : List Nat → Forest → Forest → Tree
  -- Just x: it holds both sides.
  | [], l, r => .node x l r
  -- x, then the run that starts at y as x's only right child, which holds r.
  | y :: ys, l, r => .node x l (.cons (runTree y ys .nil r) .nil)

-- Promise: a run reads as its left children, its characters in order, then
-- its right children.
theorem run_trav (x : Nat) (xs : List Nat) (l r : Forest) :
    (runTree x xs l r).trav = l.trav ++ x :: (xs ++ r.trav) := by
  -- Go down the run one character at a time, for any first character and left side.
  induction xs generalizing x l with
  -- Just x: the node reads l, x, r.
  | nil => simp [runTree, Tree.trav]
  -- x, then the run from y: x's right side is that run, which reads y, ys, r.
  | cons y ys ih => simp [runTree, Tree.trav, Forest.trav, ih]

-- Promise: cutting the run x, a, y, b after a gives the same tree: the first
-- piece is x, a with the left children, and its only right child is the piece
-- y, b holding the old right children.
theorem split_run (x y : Nat) (a b : List Nat) (l r : Forest) :
    runTree x (a ++ y :: b) l r = runTree x a l (.cons (runTree y b .nil r) .nil) := by
  -- Go down the first piece one character at a time, for any start and left side.
  induction a generalizing x l with
  -- Nothing between x and the cut: y's run is x's only right child either way.
  | nil => rfl
  -- x, then c: both sides hang the run from c under x, and the cut is inside it.
  | cons c cs ih => simp [runTree, ih]

end TextCrdt

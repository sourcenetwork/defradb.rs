/-!
# TextCrdt.Fugue: where a character goes, and why the order of arrival does not matter

The story, in plain words. A Text field is a tree of characters (a Fugue
tree). Every character is a node; each node has a list of left children and a
list of right children. Reading the tree left to right (left children, the
node, right children, each child with everything under it) gives the
characters in order. Deleted characters stay in the tree, marked dead, and are
skipped when the text is shown.

An edit says "insert e right after the character lo" (lo is the character at
position p-1 of the edit's parent version; the root stands for position 0).
Fugue hangs e as the leftmost node of lo's right children: if lo has no right
child, e becomes its only one; otherwise e becomes the left child of the
leftmost node under lo's first right child. Siblings that end up under the
same node on the same side are kept sorted by their block ID.

What must never happen:
* the inserted character lands anywhere but right after lo, or anything else
  moves (`place_visible`, built on `trav_place`);
* a delete removes anything but its own character (`delete_visible`);
* two peers that hang the same two concurrent characters in opposite orders
  end up with different trees (`attach_comm`).

The temporal story (peers swapping blocks in any order, a block waiting for
its parents, positions read in the parent version) is the TLA+ model
`proofs/tla/TextCrdt.tla`; this file carries the tree laws that model checks
only on small cases.

Rust anchors: the Text CRDT is not written yet. This file is its contract
(issue #2127), for `crates/crdt/src/text.rs` (Fugue placement, sibling order
by CID, replay) and `crates/db/src/merge/merge_handler/text.rs` (merging a
received block).
-/

namespace TextCrdt

mutual
  -- A character with everything hung under it.
  inductive Tree where
    -- Its ID, its left children, its right children.
    | node : Nat → Forest → Forest → Tree
  -- A list of sibling trees, in reading order.
  inductive Forest where
    -- No siblings.
    | nil : Forest
    -- One sibling, then the rest.
    | cons : Tree → Forest → Forest
end

-- The ID of the character at the top of a tree.
def Tree.id : Tree → Nat
  -- The node's own ID.
  | .node i _ _ => i

-- A character with nothing under it.
def leaf (e : Nat) : Tree :=
  -- No left children and no right children.
  .node e .nil .nil

mutual
  -- Reading a tree left to right: its left children, itself, its right children.
  def Tree.trav : Tree → List Nat
    -- Everything left of i, then i, then everything right of i.
    | .node i l r => l.trav ++ i :: r.trav
  -- Reading siblings left to right, each with everything under it.
  def Forest.trav : Forest → List Nat
    -- No siblings: nothing to read.
    | .nil => []
    -- The first sibling, then the rest.
    | .cons t f => t.trav ++ f.trav
end

mutual
  -- Hang e as the leftmost node of a tree: the left child of its leftmost node.
  def Tree.addLeftmost (e : Nat) : Tree → Tree
    -- Go into the left children; the node and its right side stay.
    | .node i l r => .node i (l.addLeftmost e) r
  -- Hang e as the leftmost node of a list of siblings.
  def Forest.addLeftmost (e : Nat) : Forest → Forest
    -- No siblings: e becomes the only one.
    | .nil => .cons (leaf e) .nil
    -- Otherwise go into the first sibling; the rest stay.
    | .cons t f => .cons (t.addLeftmost e) f
end

mutual
  -- Promise: hanging e leftmost puts e first when the tree is read.
  theorem Tree.trav_addLeftmost (e : Nat) : ∀ t : Tree, (t.addLeftmost e).trav = e :: t.trav
    -- The left children read e first, so the whole tree does.
    | .node i l r => by simp [Tree.addLeftmost, Tree.trav, Forest.trav_addLeftmost e l]
  -- Promise: hanging e leftmost in siblings puts e first when they are read.
  theorem Forest.trav_addLeftmost (e : Nat) : ∀ f : Forest, (f.addLeftmost e).trav = e :: f.trav
    -- No siblings: the only one read is e.
    | .nil => by simp [Forest.addLeftmost, Forest.trav, Tree.trav, leaf]
    -- The first sibling reads e first, so all of them do.
    | .cons t f => by simp [Forest.addLeftmost, Forest.trav, Tree.trav_addLeftmost e t]
end

mutual
  -- Fugue's rule: hang e as the leftmost node of lo's right children.
  def Tree.place (lo e : Nat) : Tree → Tree
    -- At each node, first place inside its children;
    | .node i l r =>
      -- if this node is lo,
      if i = lo then
        -- e goes leftmost among its right children,
        .node i (l.place lo e) ((r.place lo e).addLeftmost e)
      -- and any other node is left as it is.
      else .node i (l.place lo e) (r.place lo e)
  -- Fugue's rule applied inside each of a list of siblings.
  def Forest.place (lo e : Nat) : Forest → Forest
    -- No siblings: nothing to do.
    | .nil => .nil
    -- Place inside the first sibling and inside the rest.
    | .cons t f => .cons (t.place lo e) (f.place lo e)
end

-- What placing does to the reading, one character at a time: lo is followed by e.
def subst (lo e x : Nat) : List Nat :=
  -- lo becomes lo then e; every other character stays as it is.
  if x = lo then [lo, e] else [x]

mutual
  -- Promise: after placing, the tree reads exactly as before with e right after lo.
  theorem Tree.trav_place (lo e : Nat) :
      ∀ t : Tree, (t.place lo e).trav = t.trav.flatMap (subst lo e)
    -- At a node, the children read as promised, and the node itself is lo or not.
    | .node i l r => by
      -- What the children read after placing.
      have hl := Forest.trav_place lo e l
      -- The same for the right children.
      have hr := Forest.trav_place lo e r
      -- Split on whether this node is lo.
      by_cases h : i = lo
      -- It is: lo, then e, then its old right side.
      · simp [Tree.place, Tree.trav, h, hl, hr, Forest.trav_addLeftmost, subst, List.flatMap_append]
      -- It is not: it reads as itself.
      · simp [Tree.place, Tree.trav, h, hl, hr, subst, List.flatMap_append]
  -- Promise: after placing, siblings read as before with e right after lo.
  theorem Forest.trav_place (lo e : Nat) :
      ∀ f : Forest, (f.place lo e).trav = f.trav.flatMap (subst lo e)
    -- No siblings: nothing read before or after.
    | .nil => by simp [Forest.place, Forest.trav]
    -- The first sibling and the rest each read as promised.
    | .cons t f => by
      simp [Forest.place, Forest.trav, Tree.trav_place lo e t, Forest.trav_place lo e f,
        List.flatMap_append]
end

-- The characters shown: the reading with the dead ones skipped.
def visible (dead : Nat → Bool) (l : List Nat) : List Nat :=
  -- Keep a character only if it is not dead.
  l.filter (fun x => !dead x)

-- Promise: a reading without lo in it is untouched by lo's substitution.
theorem flatMap_subst_of_not_mem (lo e : Nat) :
    ∀ l : List Nat, lo ∉ l → l.flatMap (subst lo e) = l
  -- Nothing to substitute in an empty reading.
  | [] => fun _ => rfl
  -- The first character is not lo, so it stays, and so does the rest.
  | x :: l => fun h => by
    -- x is not lo, and lo is not in the rest.
    simp only [List.mem_cons, not_or] at h
    -- Substitute x (which stays) and the rest (which stays by the same promise).
    simp [subst, Ne.symm h.1, flatMap_subst_of_not_mem lo e l h.2]

-- Promise: in a reading where every character appears once, lo splits it into a
-- part before lo and a part after, and lo appears in neither.
theorem split_at (lo : Nat) (l : List Nat) (hnd : l.Nodup) (hlo : lo ∈ l) :
    ∃ A B, l = A ++ lo :: B ∧ lo ∉ A ∧ lo ∉ B := by
  -- lo is somewhere in the reading.
  obtain ⟨A, B, h⟩ := List.append_of_mem hlo
  -- Every character appears once in A ++ lo :: B.
  rw [h] at hnd
  -- So no character of A equals one of lo :: B, and lo :: B has no repeats.
  have hp := List.pairwise_append.mp hnd
  -- Hand back the split.
  refine ⟨A, B, h, ?_, ?_⟩
  -- lo in A would equal the lo that starts lo :: B.
  · exact fun hA => hp.2.2 lo hA lo (List.mem_cons_self _ _) rfl
  -- lo in B would repeat lo inside lo :: B.
  · exact (List.nodup_cons.mp hp.2.1).1

-- Promise (intention preservation): placing e after a character lo of a tree whose
-- characters each appear once shows e right after lo and moves nothing else:
-- the text before is X ++ Y, the text after is X ++ e :: Y, where X is
-- everything up to and including lo.
theorem place_visible (t : Tree) (lo e : Nat) (dead : Nat → Bool)
    (hnd : t.trav.Nodup) (hlo : lo ∈ t.trav) (he : dead e = false) :
    ∃ A B, t.trav = A ++ lo :: B ∧
      visible dead (t.place lo e).trav = visible dead (A ++ [lo]) ++ e :: visible dead B ∧
      visible dead t.trav = visible dead (A ++ [lo]) ++ visible dead B := by
  -- Split the reading at lo.
  obtain ⟨A, B, h, hA, hB⟩ := split_at lo t.trav hnd hlo
  -- Use that split.
  refine ⟨A, B, h, ?_, ?_⟩
  -- After placing: A, lo, e, B, then skip the dead ones; e is alive.
  · rw [Tree.trav_place, h]
    simp [List.flatMap_append, flatMap_subst_of_not_mem lo e A hA,
      flatMap_subst_of_not_mem lo e B hB, subst, visible, List.filter_append, he]
    -- lo is shown or not; either way e comes right after where it sits.
    cases hd : dead lo <;> simp [List.filter_cons, hd, he]
  -- Before placing: A, lo, B, then skip the dead ones.
  · rw [h]
    simp [visible, List.filter_append]
    -- lo is shown or not; either way it sits between A and B.
    cases hd : dead lo <;> simp [List.filter_cons, hd]

-- Promise: at position 0 (lo is the root, which is never shown and has no left
-- children) e becomes the first character shown and nothing else moves.
theorem place_root_first (r : Forest) (e : Nat) (dead : Nat → Bool)
    (hroot : dead 0 = true) (hnd : (Tree.node 0 .nil r).trav.Nodup) (he : dead e = false) :
    visible dead ((Tree.node 0 .nil r).place 0 e).trav =
      e :: visible dead (Tree.node 0 .nil r).trav := by
  -- The root reads first, so nothing comes before it.
  have hr : (0 : Nat) ∉ r.trav := by
    -- The reading is 0 followed by the right side, with no repeats.
    simp [Tree.trav, Forest.trav] at hnd
    exact hnd.1
  -- Placing at the root: e goes right after it; the root itself is not shown.
  rw [Tree.trav_place]
  simp [Tree.trav, Forest.trav, subst, flatMap_subst_of_not_mem 0 e r.trav hr, visible,
    hroot, he]

-- Promise: deleting the shown character x hides exactly x: the text before is
-- X ++ x :: Y and the text after is X ++ Y.
theorem delete_visible (l : List Nat) (x : Nat) (dead : Nat → Bool)
    (hnd : l.Nodup) (hx : x ∈ l) (halive : dead x = false) :
    ∃ X Y, visible dead l = X ++ x :: Y ∧
      visible (fun y => dead y || y == x) l = X ++ Y := by
  -- Split the reading at x.
  obtain ⟨A, B, h, hA, hB⟩ := split_at x l hnd hx
  -- The text is what A shows, x, and what B shows.
  refine ⟨visible dead A, visible dead B, ?_, ?_⟩
  -- Before: x is alive, so it is shown between A and B.
  · rw [h]
    simp [visible, List.filter_append, halive]
  -- After: x is dead; A and B hold no x, so they show what they showed.
  · rw [h]
    -- In A, marking x dead changes nothing, because x is not there.
    have hA' : A.filter (fun y => !(dead y || y == x)) = A.filter (fun y => !dead y) :=
      List.filter_congr (fun y hy => by
        -- y is in A and x is not, so y is not x.
        have : y ≠ x := fun e => hA (e ▸ hy)
        simp [this])
    -- In B, the same.
    have hB' : B.filter (fun y => !(dead y || y == x)) = B.filter (fun y => !dead y) :=
      List.filter_congr (fun y hy => by
        -- y is in B and x is not, so y is not x.
        have : y ≠ x := fun e => hB (e ▸ hy)
        simp [this])
    -- x itself is now dead and skipped.
    simp [visible, List.filter_append, halive]
    -- Write "not (dead or x)" as "not dead and not x", the shape the goal has.
    simp only [Bool.not_or] at hA' hB'
    -- A and B show what they showed.
    rw [hA', hB']

-- Add t to a list of siblings kept sorted by ID, smallest first.
def insSorted (t : Tree) : Forest → Forest
  -- No siblings: t is the only one.
  | .nil => .cons t .nil
  -- t goes before u if its ID is smaller, otherwise after u, among the rest.
  | .cons u f => if t.id < u.id then .cons t (.cons u f) else .cons u (insSorted t f)

-- Promise: adding two siblings with different IDs gives the same list in either order.
theorem insSorted_comm (a b : Tree) (h : a.id ≠ b.id) :
    ∀ f : Forest, insSorted a (insSorted b f) = insSorted b (insSorted a f)
  -- Into no siblings: the smaller ID goes first either way.
  | .nil => by
    -- Compare a with b, both ways round.
    by_cases hab : a.id < b.id
    · have : ¬ b.id < a.id := by omega
      simp [insSorted, hab, this]
    · have : b.id < a.id := by omega
      simp [insSorted, hab, this]
  -- Into u then the rest: compare each with u and with each other.
  | .cons u f => by
    -- The promise for the rest of the list.
    have ih := insSorted_comm a b h f
    -- Every way a, b and u can compare.
    by_cases hau : a.id < u.id <;> by_cases hbu : b.id < u.id <;>
      by_cases hab : a.id < b.id <;> by_cases hba : b.id < a.id <;>
      simp [insSorted, hau, hbu, hab, hba, ih] <;> omega

mutual
  -- Hang e under node p, on its left side if `left` and its right side otherwise,
  -- in ID order among the siblings already there.
  def Tree.attach (p : Nat) (left : Bool) (e : Nat) : Tree → Tree
    -- At each node, first hang inside its children;
    | .node i l r =>
      -- if this node is p,
      if i = p then
        -- e joins its left children or its right children in ID order,
        if left then .node i (insSorted (leaf e) (l.attach p left e)) (r.attach p left e)
        else .node i (l.attach p left e) (insSorted (leaf e) (r.attach p left e))
      -- and any other node is left as it is.
      else .node i (l.attach p left e) (r.attach p left e)
  -- Hanging e under p, inside each of a list of siblings.
  def Forest.attach (p : Nat) (left : Bool) (e : Nat) : Forest → Forest
    -- No siblings: nothing to do.
    | .nil => .nil
    -- Inside the first sibling and inside the rest.
    | .cons t f => .cons (t.attach p left e) (f.attach p left e)
end

-- Promise: hanging something under a tree does not change the ID at its top.
theorem Tree.id_attach (p : Nat) (left : Bool) (e : Nat) (t : Tree) :
    (t.attach p left e).id = t.id := by
  -- Look at the top node.
  cases t with
  -- Whichever branch attach takes, the top stays the same node.
  | node i l r => by_cases h : i = p <;> cases left <;> simp [Tree.attach, Tree.id, h]

-- Promise: hanging e under p leaves a lone character alone unless it is p.
theorem leaf_attach (p : Nat) (left : Bool) (e x : Nat) (h : x ≠ p) :
    (leaf x).attach p left e = leaf x := by
  -- The lone character is not p and has no children to go into.
  simp [leaf, Tree.attach, Forest.attach, h]

-- Promise: hanging e under p inside a sorted list of siblings is the same as
-- adding the new sibling after hanging e inside both.
theorem attach_insSorted (p : Nat) (left : Bool) (e : Nat) (t : Tree) :
    ∀ f : Forest, (insSorted t f).attach p left e =
      insSorted (t.attach p left e) (f.attach p left e)
  -- No siblings: one sibling, hung inside either way.
  | .nil => by simp [insSorted, Forest.attach]
  -- Hanging inside keeps every ID, so the sorting decision is the same.
  | .cons u f => by
    -- Compare t with u.
    by_cases h : t.id < u.id
    -- t goes first, before and after hanging inside.
    · simp [insSorted, Forest.attach, h, Tree.id_attach]
    -- t goes after u, before and after hanging inside; the rest by the same promise.
    · simp [insSorted, Forest.attach, h, Tree.id_attach, attach_insSorted p left e t f]

mutual
  -- Promise (order of arrival does not matter): hanging two concurrent characters
  -- a and b gives the same tree in either order. Concurrent means different
  -- IDs, and neither is hung under the other.
  theorem Tree.attach_comm (pa pb ea eb : Nat) (sa sb : Bool)
      (hab : ea ≠ eb) (hpa : pa ≠ eb) (hpb : pb ≠ ea) :
      ∀ t : Tree, (t.attach pa sa ea).attach pb sb eb = (t.attach pb sb eb).attach pa sa ea
    -- At a node: the children agree by the same promise, then this node is pa, pb, both or neither.
    | .node i l r => by
      -- The left children agree either order.
      have hl := Forest.attach_comm pa pb ea eb sa sb hab hpa hpb l
      -- The right children agree either order.
      have hr := Forest.attach_comm pa pb ea eb sa sb hab hpa hpb r
      -- Two different lone characters have different IDs.
      have hid : (leaf ea).id ≠ (leaf eb).id := by simp [leaf, Tree.id, hab]
      -- Which node each hangs under: this one or not, for a and for b.
      by_cases hia : i = pa <;> by_cases hib : i = pb
      -- Both under this node: pa and pb are the same node, and the sides decide the rest.
      · have hp : pb = pa := hib.symm.trans hia
        -- Write pa for pb everywhere.
        subst hp
        -- Every pair of sides; on the same side the sorted list takes both in either order.
        cases sa <;> cases sb <;>
          simp [Tree.attach, hia, attach_insSorted, leaf_attach, hpa, hpb, Ne.symm hpa,
            Ne.symm hpb, hl, hr, insSorted_comm _ _ hid]
      -- Only a under this node, b deeper: each side as it falls.
      · subst hia
        cases sa <;> cases sb <;>
          simp [Tree.attach, hib, attach_insSorted, leaf_attach, hpa, hpb, Ne.symm hpa,
            Ne.symm hpb, hl, hr]
      -- Only b under this node, a deeper: each side as it falls.
      · subst hib
        cases sa <;> cases sb <;>
          simp [Tree.attach, hia, attach_insSorted, leaf_attach, hpa, hpb, Ne.symm hpa,
            Ne.symm hpb, hl, hr]
      -- Neither under this node: only the children change, and they agree.
      · cases sa <;> cases sb <;> simp [Tree.attach, hia, hib, hl, hr]
  -- Promise: the same for a list of siblings.
  theorem Forest.attach_comm (pa pb ea eb : Nat) (sa sb : Bool)
      (hab : ea ≠ eb) (hpa : pa ≠ eb) (hpb : pb ≠ ea) :
      ∀ f : Forest, (f.attach pa sa ea).attach pb sb eb = (f.attach pb sb eb).attach pa sa ea
    -- No siblings: nothing to hang.
    | .nil => by simp [Forest.attach]
    -- The first sibling and the rest each agree.
    | .cons t f => by
      simp [Forest.attach, Tree.attach_comm pa pb ea eb sa sb hab hpa hpb t,
        Forest.attach_comm pa pb ea eb sa sb hab hpa hpb f]
end

end TextCrdt

import TextCrdt.Span
/-!
# TextCrdt.Compact: folding the tree down to its text changes no later edit

The story, in plain words. Replaying a Text field's whole history builds a
Fugue tree that keeps every character ever typed, the deleted ones too, so it
grows with the history, not with the text. To keep it small, the replay
stops at a critical block (one every other block comes before or after) and
throws the tree away, keeping only the text it shows: one run of those
characters under the root. Every block after a critical block comes after it,
so it sees all of the old tree; the replay goes on from the run.

Who acts: a replay that applies edits one after another. An edit either
hangs a new character right after a character that is shown (or right after
the root, at position 0), or deletes a character that is shown.

What must never happen: the text after the edits that follow differs
between the old tree and the folded one. `run_compact` promises it never
does: two trees that show the same text keep showing the same text under any
sequence of such edits, and `compact_visible` promises the folded tree shows
the old tree's text. Underneath sit two plain facts: what an insert after a
shown character does to the text depends only on the text (`visible_place`,
`visible_place_root`), and so does what a delete does (`visible_kill`).

Blocks after the critical one that are concurrent with each other also read
positions in versions that leave each other out; that temporal side is
`INV_CriticalReplay` in `proofs/tla/TextCrdt.tla`, checked from a critical
block's text kept as a chain of characters, which `TextCrdt.Span` shows is the
span the code keeps.

Rust anchors, in `crates/building-blocks/crdt/src/fugue_checkpoint.rs`:
`replay` folds the tree at critical blocks (`critical_steps`) through
`Tree::checkpoint`, which builds the run (`compact`); `Tree::text` reads the
text as `visible` does, a span piece taking its characters from the run.
-/

namespace TextCrdt

-- Promise: reading what was already read, skipping the dead, skips nothing more.
theorem visible_visible (dead : Nat → Bool) (l : List Nat) :
    visible dead (visible dead l) = visible dead l := by
  -- Filtering twice by the same test is filtering once.
  simp [visible, List.filter_filter]

-- Promise: when lo and e are both alive, putting e after lo and then skipping
-- the dead is the same as skipping the dead and then putting e after lo.
theorem visible_flatMap_subst (dead : Nat → Bool) (lo e : Nat)
    (hlo : dead lo = false) (he : dead e = false) :
    ∀ l : List Nat, visible dead (l.flatMap (subst lo e)) = (visible dead l).flatMap (subst lo e)
  -- Nothing read: nothing either way.
  | [] => by simp [visible]
  -- The first character x, then the rest by the same promise.
  | x :: l => by
    -- The rest of the reading, handled the same way.
    have ih := visible_flatMap_subst dead lo e hlo he l
    -- x is lo, or it is not.
    by_cases hx : x = lo
    -- It is lo: lo and e are both shown, before the rest.
    · subst hx
      simp [visible, subst, hlo, he, List.filter_cons] at ih ⊢
      exact ih
    -- It is not lo: x stays as it is, shown or not.
    · cases hd : dead x <;> simp [visible, subst, hx, hd, List.filter_cons] at ih ⊢ <;> exact ih

-- Promise: inserting e after a shown character lo changes the text the same way
-- whatever the tree looks like: the text gets e right after lo.
theorem visible_place (t : Tree) (lo e : Nat) (dead : Nat → Bool)
    (hlo : dead lo = false) (he : dead e = false) :
    visible dead (t.place lo e).trav = (visible dead t.trav).flatMap (subst lo e) := by
  -- Placing reads as the old reading with e after lo; the dead skip as before.
  rw [Tree.trav_place, visible_flatMap_subst dead lo e hlo he]

-- Promise: deleting x changes the text the same way whatever the tree looks
-- like: x leaves the text.
theorem visible_kill (l : List Nat) (x : Nat) (dead : Nat → Bool) :
    visible (fun y => dead y || y == x) l = (visible dead l).filter (fun y => y != x) := by
  -- Skipping the dead and x is skipping the dead, then skipping x.
  simp only [visible, List.filter_filter, Bool.not_or]
  -- The two tests agree on every character: the same two checks, swapped.
  exact List.filter_congr (fun y _ => Bool.and_comm _ _)

-- A tree is rooted: the root 0 is on top with no left children, and appears
-- nowhere under it. The code's tree always is: nothing hangs left of the root.
def Rooted (t : Tree) : Prop :=
  -- Some right side r under the root, with no root inside it.
  ∃ r, t = .node 0 .nil r ∧ 0 ∉ r.trav

-- Promise: a character of a reading with e put after lo is a character of the
-- old reading, or e.
theorem mem_flatMap_subst (lo e y : Nat) (l : List Nat)
    (h : y ∈ l.flatMap (subst lo e)) : y ∈ l ∨ y = e := by
  -- y came from substituting some x of the old reading.
  obtain ⟨x, hx, hy⟩ := List.mem_flatMap.mp h
  -- x was lo, giving lo then e, or something else, giving itself.
  by_cases hxl : x = lo
  -- y is lo (then x, in the reading) or e.
  · simp [subst, hxl] at hy
    rcases hy with hy | hy
    · exact Or.inl (hy ▸ hxl ▸ hx)
    · exact Or.inr hy
  -- y is x itself, which is in the reading.
  · simp [subst, hxl] at hy
    exact Or.inl (hy ▸ hx)

-- Promise: inserting a character that is not the root keeps a tree rooted.
theorem rooted_place (t : Tree) (lo e : Nat) (he0 : e ≠ 0) (ht : Rooted t) :
    Rooted (t.place lo e) := by
  -- The root and its right side.
  obtain ⟨r, rfl, hr⟩ := ht
  -- Placing reads, inside r, as r with e after lo; neither brings back a root.
  have hnot : ∀ l : List Nat, 0 ∉ l → 0 ∉ l.flatMap (subst lo e) := fun l hl h0 =>
    (mem_flatMap_subst lo e 0 l h0).elim hl (fun h => he0 h.symm)
  -- The root is lo, or it is not.
  by_cases h : (0 : Nat) = lo
  -- e goes first under the root: its right side reads e, then r with e after lo.
  · refine ⟨((r.place lo e).addLeftmost e), by simp [Tree.place, Forest.place, h], ?_⟩
    rw [Forest.trav_addLeftmost, Forest.trav_place]
    simp only [List.mem_cons, not_or]
    exact ⟨fun h0 => he0 h0.symm, hnot r.trav hr⟩
  -- Only the right side changes.
  · refine ⟨r.place lo e, by simp [Tree.place, Forest.place, h], ?_⟩
    rw [Forest.trav_place]
    exact hnot r.trav hr

-- Promise: inserting e at position 0 of a rooted tree puts e first in the
-- text, whatever the tree looks like.
theorem visible_place_root (t : Tree) (e : Nat) (dead : Nat → Bool)
    (hroot : dead 0 = true) (he : dead e = false) (ht : Rooted t) :
    visible dead (t.place 0 e).trav = e :: visible dead t.trav := by
  -- The root and its right side.
  obtain ⟨r, rfl, hr⟩ := ht
  -- e goes first under the root, and r has no root in it, so nothing else moves.
  simp [Tree.place, Forest.place, Tree.trav, Forest.trav, Forest.trav_addLeftmost,
    Forest.trav_place, flatMap_subst_of_not_mem 0 e r.trav hr, visible, hroot, he]

-- One edit of a replay that sees the whole tree: insert e after lo (lo = 0 is
-- position 0), or delete x.
inductive Edit where
  -- Hang e right after lo.
  | ins : Nat → Nat → Edit
  -- Mark x dead.
  | del : Nat → Edit

-- An edit a writer can make: an insert hangs a new, live, non-root character
-- after the root or after a shown character. A delete may name anything.
def Edit.ok (dead : Nat → Bool) : Edit → Prop
  -- e is new and live, and lo is the root or shown.
  | .ins lo e => e ≠ 0 ∧ dead e = false ∧ (lo = 0 ∨ dead lo = false)
  -- Any character.
  | .del _ => True

-- The tree and the dead characters after a list of edits.
def run : List Edit → Tree → (Nat → Bool) → Tree × (Nat → Bool)
  -- No edits: as it was.
  | [], t, dead => (t, dead)
  -- An insert changes the tree, then the rest run.
  | .ins lo e :: es, t, dead => run es (t.place lo e) dead
  -- A delete marks x dead, then the rest run.
  | .del x :: es, t, dead => run es t (fun y => dead y || y == x)

-- Every edit of the list is one a writer can make when it comes.
def Okay : List Edit → (Nat → Bool) → Prop
  -- No edits: nothing to check.
  | [], _ => True
  -- An insert: fine now, and the rest fine with the same dead characters.
  | .ins lo e :: es, dead => Edit.ok dead (.ins lo e) ∧ Okay es dead
  -- A delete: the rest fine once x is dead.
  | .del x :: es, dead => Okay es (fun y => dead y || y == x)

-- Promise: two rooted trees that show the same text still show the same text
-- after the same edits.
theorem run_same (es : List Edit) :
    ∀ (t u : Tree) (dead : Nat → Bool), dead 0 = true → Rooted t → Rooted u →
      visible dead t.trav = visible dead u.trav → Okay es dead →
      visible (run es t dead).2 (run es t dead).1.trav =
        visible (run es u dead).2 (run es u dead).1.trav := by
  -- One edit at a time, for any trees and dead characters.
  induction es with
  -- No edits: the texts were the same.
  | nil => intro t u dead _ _ _ hsame _; simpa [run] using hsame
  -- The first edit, then the rest by the same promise.
  | cons ed es ih =>
    intro t u dead hroot ht hu hsame hok
    cases ed with
    -- An insert after lo.
    | ins lo e =>
      -- It is a fair insert, and the rest are fair.
      obtain ⟨⟨he0, he, hlo⟩, hrest⟩ := hok
      -- After it, both trees are still rooted and show the same text.
      refine ih (t.place lo e) (u.place lo e) dead hroot (rooted_place t lo e he0 ht)
        (rooted_place u lo e he0 hu) ?_ hrest
      rcases hlo with hlo | hlo
      -- At position 0: e first in both.
      · subst hlo
        rw [visible_place_root t e dead hroot he ht, visible_place_root u e dead hroot he hu,
          hsame]
      -- After a shown character: e after it in both.
      · rw [visible_place t lo e dead hlo he, visible_place u lo e dead hlo he, hsame]
    -- A delete of x: x leaves the same text in both.
    | del x =>
      refine ih t u (fun y => dead y || y == x) (by simp [hroot]) ht hu ?_ hok
      rw [visible_kill t.trav x dead, visible_kill u.trav x dead, hsame]

-- The tree a run of characters makes: hanging one after the other.
def runForest : List Nat → Forest
  -- No characters: nothing under the root.
  | [] => .nil
  -- x, then the rest, each the right child of the one before.
  | x :: xs => .cons (runTree x xs .nil .nil) .nil

-- The folded tree: the root, and the shown characters of t as one run under it.
def compact (t : Tree) (dead : Nat → Bool) : Tree :=
  -- The root on top, its right side the run.
  .node 0 .nil (runForest (visible dead t.trav))

-- Promise: a run of characters reads as those characters.
theorem runForest_trav (l : List Nat) : (runForest l).trav = l := by
  -- No characters, or a run from the first one.
  cases l with
  -- Nothing to read.
  | nil => rfl
  -- The run reads x and the rest (`run_trav`).
  | cons x xs => simp [runForest, Forest.trav, run_trav]

-- Promise: the folded tree shows exactly the text the old tree shows.
theorem compact_visible (t : Tree) (dead : Nat → Bool) (hroot : dead 0 = true) :
    visible dead (compact t dead).trav = visible dead t.trav := by
  -- The root is dead and skipped; the run's characters are all shown.
  -- The root is dead and skipped, and the run reads the old text.
  have h : visible dead (compact t dead).trav = visible dead (visible dead t.trav) := by
    simp [compact, Tree.trav, Forest.trav, runForest_trav, visible, hroot]
  -- Skipping the dead in a text that has none skips nothing.
  rw [h, visible_visible]

-- Promise: the folded tree is rooted, because the root is dead, so it is not
-- among the characters shown.
theorem compact_rooted (t : Tree) (dead : Nat → Bool) (hroot : dead 0 = true) :
    Rooted (compact t dead) := by
  -- Its right side is the run, which holds only shown characters.
  refine ⟨_, rfl, ?_⟩
  rw [runForest_trav]
  simp [visible, hroot]

-- Promise (the fold changes nothing): replaying any fair edits after folding a
-- rooted tree down to its text shows the same text as replaying them on the
-- tree itself.
theorem run_compact (es : List Edit) (t : Tree) (dead : Nat → Bool)
    (hroot : dead 0 = true) (ht : Rooted t) (hok : Okay es dead) :
    visible (run es (compact t dead) dead).2 (run es (compact t dead) dead).1.trav =
      visible (run es t dead).2 (run es t dead).1.trav :=
  -- Both start rooted and show the same text, so they agree after every edit.
  run_same es (compact t dead) t dead hroot (compact_rooted t dead hroot) ht
    (compact_visible t dead hroot) hok

end TextCrdt

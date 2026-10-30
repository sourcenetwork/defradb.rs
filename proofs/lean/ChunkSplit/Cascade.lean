import Std

/-!
# ChunkSplit.Cascade: deleting a collection deletes its chunk collections

The story, in plain words.

* Who acts: an operator deletes a collection, say `Article`, whose `body` is
  `@chunk`. The node holds `Article__body_chunk` beside it.
* What can happen: the delete removes the collection, and with it every
  collection generated from it.
* What must never happen: a chunk collection left behind whose parent is gone.
  Adding `Article` again would then meet an old `Article__body_chunk` already
  holding the name.

Rust anchors:
* `crates/db/src/collection/ops/delete.rs` `delete_collection_with_txn`: deletes
  the collection and then each of its chunk collections. Modeled by `delete`.
* The same file's `delete_collections_as` adds each parent's chunk collections
  to the names it deletes. Modeled by `delete` too.
* The bug it replaced, deleting only the named collection, is `deleteAlone`.

Verify: `cd proofs/lean && lake build ChunkSplit`
-/

namespace ChunkSplit.Cascade

-- A collection: its name, and the parent it holds chunks of, if any.
structure Col where
  -- The collection's name, modeled by a number.
  name : Nat
  -- For a chunk collection, the name of its parent.
  chunkOf : Option Nat
  -- Two collections can be compared for equality.
  deriving DecidableEq, Repr

-- Whether `c` goes when `parent` is deleted: it is the parent, or one of its chunk collections.
def goesWith (parent : Nat) (c : Col) : Bool :=
  -- The named collection itself, or a collection generated from it.
  c.name == parent || c.chunkOf == some parent

-- What the fixed delete keeps: every collection that doesn't go with `parent`.
def delete (parent : Nat) (cols : List Col) : List Col :=
  -- Drop the parent and its chunk collections in one step.
  cols.filter (fun c => !goesWith parent c)

-- What the old delete kept: everything but the named collection.
def deleteAlone (parent : Nat) (cols : List Col) : List Col :=
  -- Only the parent itself is dropped.
  cols.filter (fun c => c.name != parent)

/-- After the delete, no chunk collection of the deleted parent is left. -/
theorem no_orphan_chunks (parent : Nat) (cols : List Col) :
    ∀ c ∈ delete parent cols, c.chunkOf ≠ some parent := by
  -- Take a kept collection.
  intro c hc
  -- Being kept means it doesn't go with the parent.
  simp only [delete, List.mem_filter, goesWith, Bool.not_eq_true', Bool.or_eq_false_iff,
    beq_eq_false_iff_ne] at hc
  -- So it isn't one of the parent's chunk collections.
  exact hc.2.2

/-- The parent itself is gone too. -/
theorem parent_is_gone (parent : Nat) (cols : List Col) :
    ∀ c ∈ delete parent cols, c.name ≠ parent := by
  -- Take a kept collection.
  intro c hc
  -- Being kept means it isn't the parent.
  simp only [delete, List.mem_filter, goesWith, Bool.not_eq_true', Bool.or_eq_false_iff,
    beq_eq_false_iff_ne] at hc
  -- Read off the first half.
  exact hc.2.1

/-- Every other collection stays: the delete takes nothing it shouldn't. -/
theorem others_stay (parent : Nat) (cols : List Col) (c : Col) (hc : c ∈ cols)
    (hname : c.name ≠ parent) (hchunk : c.chunkOf ≠ some parent) :
    c ∈ delete parent cols := by
  -- It is in the list and doesn't go with the parent.
  simp [delete, goesWith, hc, hname, hchunk]

/-- The old delete left the chunk collection behind. -/
theorem delete_alone_leaves_an_orphan :
    ∃ cols : List Col, ∃ c ∈ deleteAlone 0 cols, c.chunkOf = some 0 := by
  -- `Article` is 0 and `Article__body_chunk` is 1, a chunk collection of 0.
  refine ⟨[⟨0, none⟩, ⟨1, some 0⟩], ⟨1, some 0⟩, ?_, rfl⟩
  -- Deleting 0 alone keeps 1.
  decide

end ChunkSplit.Cascade

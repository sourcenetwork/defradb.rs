/-!
# A collection never lists the same index twice

The story, in plain words. A collection keeps a list of index definitions.
A definition is what an index is: its fields, their order and directions,
and its kind (ordered and unique or not, or a vector configuration). A
caller asks for a new index with some definition. The database looks at the
definitions the collection already lists and, if one is the same, refuses
the request; otherwise it adds the new definition at the end of the list.

Some entries are hidden: the node adds an index on its own for a primary
relation's `_<name>ID` field and does not list it. Whether an entry is hidden
belongs to the entry (Rust decides it by the entry's name), not to its
definition: a hidden entry never counts as a copy, so a caller can still ask
for that index by hand, and the index the caller gets is listed, so asking
for it a second time is refused.

What must never happen: two listed definitions that are the same. What must
also never happen: a request refused although nothing listed matches it
(nothing valid is rejected).

Rust: `crates/database/db/src/index/duplicate.rs` `refuse_duplicate_index`,
called from `crates/database/db/src/index/create.rs` `define_index` and
`crates/database/db/src/index/in_txn.rs` `create_index_in`, the index creation
requests; the node's `ensure_ordinary_index` reconcile API does not run it.
The definition is abstracted as any type with decidable equality, and the
name check that runs first is left out: it only refuses more.
-/
namespace DuplicateIndex

-- `α` is the type of index definitions; it only needs equality.
variable {α : Type} [DecidableEq α]

-- One entry of the collection's list: its definition and whether it is
-- hidden (the node added it on its own and does not list it).
abbrev Entry (α : Type) := α × Bool

-- The definitions a caller sees: those of the entries that are not hidden.
def listed (defs : List (Entry α)) : List α :=
  (defs.filter (fun e => !e.2)).map (·.1)

-- One create request: refused (`none`) when a listed definition equals `d`,
-- otherwise the list with a new, listed entry for `d` added at the end.
def create (defs : List (Entry α)) (d : α) : Option (List (Entry α)) :=
  if d ∈ listed defs then none else some (defs ++ [(d, false)])

-- A request is refused exactly when the collection already lists the same
-- definition: never for a new one, always for a copy.
theorem refused_iff_listed (defs : List (Entry α)) (d : α) :
    create defs d = none ↔ d ∈ listed defs := by
  -- Unfold `create` and split on whether `d` is listed.
  unfold create
  by_cases h : d ∈ listed defs <;> simp [h]

-- What a successful create lists: everything listed before, then `d`.
theorem create_listed (defs defs' : List (Entry α)) (d : α)
    (ok : create defs d = some defs') :
    listed defs' = listed defs ++ [d] := by
  -- Unfold `create`; the refused branch cannot give `some`.
  unfold create at ok
  by_cases h : d ∈ listed defs
  · simp [h] at ok
  · -- The accepted branch appends one entry that is not hidden.
    simp [h] at ok
    subst ok
    simp [listed, List.filter_append, List.map_append]

-- A created index is listed in the new list, and no entry that was there is
-- lost.
theorem created_keeps_all (defs defs' : List (Entry α)) (d : α)
    (ok : create defs d = some defs') :
    d ∈ listed defs' ∧ ∀ e, e ∈ defs → e ∈ defs' := by
  -- `d` is listed by `create_listed`; the entries come from unfolding `create`.
  refine ⟨by simp [create_listed defs defs' d ok], ?_⟩
  unfold create at ok
  by_cases h : d ∈ listed defs
  · simp [h] at ok
  · -- The accepted branch is `defs ++ [(d, false)]`, which holds all of `defs`.
    simp [h] at ok
    subst ok
    exact fun e he => by simp [he]

-- A hidden entry with the same definition never blocks a request: the node's
-- own relation index alone does not stop a caller from asking for it.
theorem hidden_does_not_refuse (defs : List (Entry α)) (d : α)
    (allHidden : ∀ e, e ∈ defs → e.2 = true) :
    create defs d ≠ none := by
  -- Nothing is listed when every entry is hidden, so `d` is not listed.
  rw [Ne, refused_iff_listed]
  unfold listed
  rw [List.filter_eq_nil_iff.mpr (fun e he => by simp [allHidden e he])]
  simp

-- Once a request succeeds, the same request again is refused: an index a
-- caller created is listed, so it blocks its copy even when a hidden entry
-- has the same definition.
theorem second_create_refused (defs defs' : List (Entry α)) (d : α)
    (ok : create defs d = some defs') :
    create defs' d = none :=
  -- `d` is listed in the new list, which is exactly what refusal needs.
  (refused_iff_listed defs' d).mpr (created_keeps_all defs defs' d ok).1

-- If the listed definitions have no repeats before a create, they have none
-- after it: a successful create never makes a copy appear.
theorem create_keeps_listed_nodup (defs defs' : List (Entry α)) (d : α)
    (nodup : (listed defs).Nodup)
    (ok : create defs d = some defs') :
    (listed defs').Nodup := by
  -- The new listed list is the old one plus `d`, and `d` was not listed.
  have h : d ∉ listed defs := fun h => by
    simp [(refused_iff_listed defs d).mpr h] at ok
  rw [create_listed defs defs' d ok]
  -- No repeats means every pair differs; the old pairs already do, and each
  -- old definition differs from `d` because `d` was not listed.
  unfold List.Nodup at nodup ⊢
  rw [List.pairwise_append]
  exact ⟨nodup, List.pairwise_singleton _ d, fun a ha b hb => by
    simp at hb
    subst hb
    intro eq
    subst eq
    exact h ha⟩

end DuplicateIndex

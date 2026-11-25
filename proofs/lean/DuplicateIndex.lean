/-!
# A collection never lists the same index twice

The story, in plain words. A collection keeps a list of index definitions.
A definition is what an index is: its fields, their order and directions,
and its kind (ordered and unique or not, or a vector configuration). A
caller asks for a new index with some definition. The database looks at the
definitions the collection already lists and, if one is the same, refuses
the request; otherwise it adds the new definition at the end of the list.

Some definitions are hidden: the node adds an index on its own for a primary
relation's `_<name>ID` field and does not list it. A hidden definition never
counts as a copy, so a caller can still ask for that index by hand.

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

-- `α` is the type of index definitions; `hidden` says which ones the node
-- added on its own and does not list.
variable {α : Type} [DecidableEq α] (hidden : α → Bool)

-- The definitions a caller sees: every one that is not hidden.
def listed (defs : List α) : List α := defs.filter (fun e => !hidden e)

-- One create request: refused (`none`) when a listed definition equals `d`,
-- otherwise the list with `d` added at the end.
def create (defs : List α) (d : α) : Option (List α) :=
  if d ∈ listed hidden defs then none else some (defs ++ [d])

-- A request is refused exactly when the collection already lists the same
-- definition: never for a new one, always for a copy.
theorem refused_iff_listed (defs : List α) (d : α) :
    create hidden defs d = none ↔ d ∈ listed hidden defs := by
  -- Unfold `create` and split on whether `d` is listed.
  unfold create
  by_cases h : d ∈ listed hidden defs <;> simp [h]

-- A created index is in the new list, and nothing that was there is lost.
theorem created_keeps_all (defs defs' : List α) (d : α)
    (ok : create hidden defs d = some defs') :
    d ∈ defs' ∧ ∀ e, e ∈ defs → e ∈ defs' := by
  -- Unfold `create`; the refused branch cannot give `some`.
  unfold create at ok
  by_cases h : d ∈ listed hidden defs
  · simp [h] at ok
  · -- The accepted branch is `defs ++ [d]`, which holds `d` and all of `defs`.
    simp [h] at ok
    subst ok
    exact ⟨by simp, fun e he => by simp [he]⟩

-- If the listed definitions have no repeats before a create, they have none
-- after it: a successful create never makes a copy appear.
theorem create_keeps_listed_nodup (defs defs' : List α) (d : α)
    (nodup : (listed hidden defs).Nodup)
    (ok : create hidden defs d = some defs') :
    (listed hidden defs').Nodup := by
  -- Unfold both definitions; the refused branch cannot give `some`.
  unfold create at ok
  by_cases h : d ∈ listed hidden defs
  · simp [h] at ok
  · simp [h] at ok
    subst ok
    -- The new listed list is the old one, plus `d` when `d` is not hidden.
    unfold listed at nodup h ⊢
    rw [List.filter_append]
    cases hd : hidden d
    · -- `d` is listed: it is appended once, and was not listed before.
      simp only [List.filter_cons, List.filter_nil, hd]
      simp only [Bool.not_false, ite_true]
      -- No repeats means every pair differs; the old pairs already do, and
      -- each old entry differs from `d` because `d` was not listed.
      unfold List.Nodup at nodup ⊢
      rw [List.pairwise_append]
      exact ⟨nodup, List.pairwise_singleton _ d, fun a ha b hb => by
        simp at hb
        subst hb
        intro eq
        subst eq
        exact h ha⟩
    · -- `d` is hidden: the listed list does not change.
      simp only [List.filter_cons, List.filter_nil, hd]
      simpa using nodup

end DuplicateIndex

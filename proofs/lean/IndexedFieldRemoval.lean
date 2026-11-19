/-!
# IndexedFieldRemoval: a patch never leaves an index reading a missing field

The story, in plain words. A collection has fields, and its indexes each
read some of those fields by name. A collection patch proposes a new list of
fields. If the patch drops a field that an index reads, the index stays (a
patch cannot change indexes), so every later write would try to read a field
the collection no longer has, and fail.

So the patch check refuses a patch when some index reads a field that the
old version had and the new one does not. An index and an encrypted index
are both just "a list of field names it reads" here.

What must never happen: an accepted patch leaves an index reading a field
the new version does not have, while every index read real fields before.
And nothing valid is refused: a patch that drops no field an index reads,
such as one that only adds fields, is always accepted.

This file proves that (`accepted_keeps_indexes_resolvable`), that a refusal
always names a dropped field some index reads (`refusal_names_a_dropped_read`),
and that a patch dropping no read field is accepted (`no_dropped_read_accepted`,
`adding_fields_accepted`).

Rust anchor: `crates/foundation/schema/src/definition_validation/index_validators.rs`
`validate_indexed_field_not_removed`, run by `validate_collection_changes`
from `crates/database/db/src/definition/patch/mod.rs` `DB::patch_collection`.
-/

namespace IndexedFieldRemoval

-- A field's name.
abbrev Name := String

-- A field is dropped when the old version had it and the new one does not.
def dropped (old new : List Name) (f : Name) : Bool :=
  -- Present before, absent after.
  old.contains f && !new.contains f

-- The check: refuse when any field any index reads was dropped.
def refuses (old new : List Name) (indexes : List (List Name)) : Bool :=
  -- Look through every index, and every field it reads.
  indexes.any (fun reads => reads.any (dropped old new))

-- Every field every index reads is one of `fields`.
def Resolvable (fields : List Name) (indexes : List (List Name)) : Prop :=
  -- For each index and each field it reads, the field exists.
  ∀ reads ∈ indexes, ∀ f ∈ reads, f ∈ fields

-- Promise: if every index read real fields before and the patch is
-- accepted, every index still reads real fields after.
theorem accepted_keeps_indexes_resolvable (old new : List Name)
    (indexes : List (List Name)) (before : Resolvable old indexes)
    (accepted : refuses old new indexes = false) : Resolvable new indexes := by
  -- Take any index and any field it reads.
  intro reads hreads f hf
  -- The field existed before, by the promise on the old version.
  have hold : f ∈ old := before reads hreads f hf
  -- Either the new version has it, or it was dropped.
  cases hm : new.contains f with
  | true => exact List.contains_iff_mem.mp hm
  | false =>
    -- Dropped: this index reads a dropped field.
    have hdrop : dropped old new f = true := by
      unfold dropped
      rw [List.contains_iff_mem.mpr hold, hm]
      rfl
    -- So the check would have refused the patch, but it accepted it.
    have hrefused : refuses old new indexes = true := by
      simp only [refuses, List.any_eq_true]
      exact ⟨reads, hreads, f, hf, hdrop⟩
    rw [accepted] at hrefused
    exact absurd hrefused Bool.false_ne_true

-- Promise: a refusal always comes from a field the patch dropped that some
-- index reads, never from anything else.
theorem refusal_names_a_dropped_read (old new : List Name)
    (indexes : List (List Name)) (h : refuses old new indexes = true) :
    ∃ reads ∈ indexes, ∃ f ∈ reads, f ∈ old ∧ f ∉ new := by
  -- Unfold the check to the index and field that made it true.
  simp only [refuses, List.any_eq_true] at h
  obtain ⟨reads, hreads, hany⟩ := h
  obtain ⟨f, hf, hdrop⟩ := hany
  -- That field was present before and absent after.
  simp [dropped, List.contains_iff_mem] at hdrop
  exact ⟨reads, hreads, f, hf, hdrop.1, hdrop.2⟩

-- Promise: a patch that drops no field any index reads is accepted.
theorem no_dropped_read_accepted (old new : List Name)
    (indexes : List (List Name))
    (h : ∀ reads ∈ indexes, ∀ f ∈ reads, f ∈ old → f ∈ new) :
    refuses old new indexes = false := by
  -- If it refused, it would name a dropped field some index reads.
  cases hr : refuses old new indexes with
  | false => rfl
  | true =>
    -- But every field an index reads that was there is still there.
    obtain ⟨reads, hreads, f, hf, hold, hnew⟩ :=
      refusal_names_a_dropped_read old new indexes hr
    exact absurd (h reads hreads f hf hold) hnew

-- Promise: a patch that only adds fields is always accepted.
theorem adding_fields_accepted (old added : List Name)
    (indexes : List (List Name)) : refuses old (old ++ added) indexes = false := by
  -- Every old field is still in the new list.
  apply no_dropped_read_accepted
  intro _ _ f _ hold
  exact List.mem_append_left added hold

end IndexedFieldRemoval

/-!
# A relation is set from its primary side only, on both write paths

The story, in plain words. A relation between two collections is stored on
one side, the primary side. Go's mutation input types leave the other side
out, so `add_Book(input: {author: ...})` on the secondary side is refused,
and Go's collection API refuses it too with "cannot set relation from
secondary side". defradb.rs has two places that decide: the mutation input
check, which first turns a relation's name into its ID field (`author` into
`_authorID`) and refuses a field that belongs to a relation on its secondary
side; and `Document::set`, which the collection API reaches, and which
refuses a relation field or a relation's ID field on its secondary side.

This file proves the two agree and do what Go does:

* a primary-side field is never refused, and neither is a plain field;
* naming the relation or naming its ID field gets the same answer;
* `Document::set` refuses everything the input check refuses, and, since
  every relation-kind field belongs to a relation, nothing more.

Rust owner: crates/query/src/runner/mutation_inputs.rs
`normalize_mutation_input_fields` and `input_field_name`;
crates/document/src/document.rs `Document::set`. The tests are the
mutation field_kinds secondary-side scenarios (ed5f5e659).
-/

namespace SecondaryRelation

-- What each check reads from a field's definition.
structure Field where
  -- The field's kind is a relation (an object or a list of objects).
  isRelationKind : Bool
  -- The field belongs to a relation: its `relation_name` is set. A relation's
  -- object field and its ID field both carry it.
  hasRelation : Bool
  -- The field is on the relation's primary side.
  isPrimary : Bool
  deriving DecidableEq, Repr

-- The mutation input check, on the field the input key names after
-- normalization.
def inputRefuses (f : Field) : Bool := f.hasRelation && !f.isPrimary

-- `Document::set`'s check.
def setRefuses (f : Field) : Bool := (f.isRelationKind || f.hasRelation) && !f.isPrimary

-- The schema's promise: a relation-kind field belongs to a relation.
def WellFormed (f : Field) : Prop := f.isRelationKind = true → f.hasRelation = true

-- A relation's object field and its ID field: same relation, same side.
def SameRelation (obj id : Field) : Prop :=
  obj.hasRelation = id.hasRelation ∧ obj.isPrimary = id.isPrimary

-- Promise: a primary-side field is never refused, by either check.
theorem primary_never_refused (f : Field) (h : f.isPrimary = true) :
    inputRefuses f = false ∧ setRefuses f = false := by
  -- `!isPrimary` is false, so both conjunctions are false.
  simp [inputRefuses, setRefuses, h]

-- Promise: a plain field, no relation of any kind, is never refused.
theorem plain_never_refused (f : Field) (hk : f.isRelationKind = false)
    (hr : f.hasRelation = false) : inputRefuses f = false ∧ setRefuses f = false := by
  -- Neither check has anything to refuse.
  simp [inputRefuses, setRefuses, hk, hr]

-- Promise: a secondary-side ID field is refused by both checks, so
-- `_authorID` cannot slip past the way it once did.
theorem secondary_id_refused (f : Field) (hr : f.hasRelation = true)
    (hp : f.isPrimary = false) : inputRefuses f = true ∧ setRefuses f = true := by
  -- Both checks see a relation on its secondary side.
  simp [inputRefuses, setRefuses, hr, hp]

-- Promise: naming the relation or naming its ID field gets the same answer.
theorem same_answer_for_relation_and_id (obj id : Field) (h : SameRelation obj id) :
    inputRefuses obj = inputRefuses id := by
  -- The input check reads only what the two fields share.
  obtain ⟨hr, hp⟩ := h
  simp [inputRefuses, hr, hp]

-- Promise: `Document::set` refuses everything the input check refuses.
theorem set_refuses_what_input_refuses (f : Field) (h : inputRefuses f = true) :
    setRefuses f = true := by
  -- Every combination of the three flags, checked.
  cases hk : f.isRelationKind <;> cases hr : f.hasRelation <;> cases hp : f.isPrimary <;>
    simp_all [inputRefuses, setRefuses]

-- Promise: on a well-formed schema the two checks are the same rule.
theorem checks_agree (f : Field) (h : WellFormed f) : setRefuses f = inputRefuses f := by
  -- The only case they could differ, a relation kind without a relation,
  -- is the one the schema rules out.
  unfold WellFormed at h
  cases hk : f.isRelationKind <;> cases hr : f.hasRelation <;> cases hp : f.isPrimary <;>
    simp_all [inputRefuses, setRefuses]

end SecondaryRelation

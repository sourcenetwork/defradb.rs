import Lean

/-! A resource with a create rule admits a document only under a parent
object. The rule names a relation of the document's resource; that relation
accepts objects of exactly one parent type, and the document's field gives the
parent's ID. The registration writes the owner and the parent link together,
and only for an identity that holds the rule's permission on that parent.
Afterwards the document keeps a parent: its last parent link cannot be
deleted, and an update cannot write another parent into its field.

The code: `RegistrationParent::from_policy` in
`crates/engine/acp/src/object_registration.rs`; `register_under_parent` and
`check_parent_unlink` in `crates/engine/acp/src/zanzibar/acp/document_acp.rs`;
`validate_immutable_fields` in `crates/database/db/src/collection/validation.rs`. -/
namespace ParentRegistration
open Lean

/-- What the policy and the document give a registration: whether the
resource has a create rule, what the rule's parent relation accepts
(`none`, `entity` for actors, `entity_set` for objects), the accepted object
type, the relation a userset would name, and the ID in the document's field. -/
structure Input where
  hasRule : Bool
  restriction : String
  resource : String
  relation : String
  id : String
  deriving FromJson, ToJson, Repr

/-- The parent object a child registration is checked against. -/
structure Parent where
  resource : String
  objectId : String
  deriving DecidableEq, FromJson, ToJson, Repr

/-- The parent a registration names: the rule's single object type with the
ID from the document, or nothing when the policy does not give exactly that. -/
def resolve (i : Input) : Option Parent :=
  -- A rule, a relation that takes whole objects, and an ID are all needed.
  if i.hasRule && i.restriction == "entity_set" && i.relation.isEmpty && !i.id.isEmpty then
    -- The policy gives the type, the document gives the ID.
    some ⟨i.resource, i.id⟩
  else
    none

/-- A resolved parent is the policy's type with the document's own ID: the
document cannot name a type, and the policy cannot name an ID. -/
theorem resolve_takes_type_from_policy_and_id_from_document (i : Input) (p : Parent)
    (h : resolve i = some p) : p.resource = i.resource ∧ p.objectId = i.id := by
  -- Open the definition; the only `some` branch builds exactly that pair.
  unfold resolve at h
  split at h
  · cases h
    exact ⟨rfl, rfl⟩
  · cases h

/-- A resource without a create rule names no parent. -/
theorem no_rule_no_parent (i : Input) (h : i.hasRule = false) : resolve i = none := by
  simp [resolve, h]

/-- A document whose field is empty names no parent. -/
theorem empty_id_no_parent (i : Input) (h : i.id.isEmpty = true) : resolve i = none := by
  simp [resolve, h]

/-- A relation that takes a userset (the members of an object's relation)
names no parent: the parent is one whole object. -/
theorem userset_no_parent (i : Input) (h : i.relation.isEmpty = false) : resolve i = none := by
  simp [resolve, h]

/-- The facts read in the transaction that registers a child. -/
structure Admission where
  -- The policy resolves a parent for this resource and ID.
  resolved : Bool
  -- The parent the caller passed is the one the policy resolves.
  parentMatches : Bool
  -- The acting identity holds the rule's permission on the parent.
  permitted : Bool
  -- The child has no owner yet.
  unregistered : Bool
  deriving FromJson, ToJson, Repr

/-- A child is registered only when every fact holds. -/
def mayRegister (a : Admission) : Bool :=
  a.resolved && a.parentMatches && a.permitted && a.unregistered

/-- Without the permission on the parent there is no registration. -/
theorem unpermitted_denied (a : Admission) (h : a.permitted = false) : mayRegister a = false := by
  simp [mayRegister, h]

/-- A parent other than the one the policy resolves is refused. -/
theorem other_parent_denied (a : Admission) (h : a.parentMatches = false) :
    mayRegister a = false := by
  simp [mayRegister, h]

/-- A policy that resolves no parent registers nothing. -/
theorem unresolved_denied (a : Admission) (h : a.resolved = false) : mayRegister a = false := by
  simp [mayRegister, h]

/-- A registration writes the owner together with its parent link, so a
registered child starts with exactly one parent. -/
def register (a : Admission) (parent : String) : Option (List String) :=
  -- The link list of the new child, or nothing when it is refused.
  if mayRegister a then some [parent] else none

/-- A registered child has a parent link from the start. -/
theorem registered_has_parent (a : Admission) (parent : String) (links : List String)
    (h : register a parent = some links) : links ≠ [] := by
  -- The only `some` branch is the one-element list.
  unfold register at h
  split at h
  · cases h
    simp
  · cases h

/-- A parent link may be deleted unless it is the only one. `links` are the
parents the child is linked to, each once, as the store keeps them. -/
def mayUnlink (links : List String) (target : String) : Bool :=
  links != [target]

/-- The links after a delete request: without the target when the delete is
allowed, untouched when it is refused. -/
def unlink (links : List String) (target : String) : List String :=
  if mayUnlink links target then links.filter (· != target) else links

/-- Deleting parent links never leaves a registered child without a parent. -/
theorem unlink_keeps_a_parent (links : List String) (target : String)
    (nonempty : links ≠ []) (nodup : links.Nodup) : unlink links target ≠ [] := by
  unfold unlink
  split
  · -- The delete was allowed, so the links are not just the target.
    rename_i allowed
    intro empty
    -- Nothing survived the filter, so every link is the target.
    have all : ∀ a ∈ links, a = target := by
      intro a mem
      have := List.filter_eq_nil_iff.mp empty a mem
      simpa using this
    match links, nonempty, nodup, all, allowed with
    | [a], _, _, all, allowed =>
      -- One link that is the target: the delete would have been refused.
      simp [mayUnlink, all a (by simp)] at allowed
    | a :: b :: rest, _, nodup, all, _ =>
      -- Two links that are both the target: the store keeps each once.
      have first := all a (by simp)
      have second := all b (by simp)
      simp [first, second] at nodup
  · -- The delete was refused, so the links are as before.
    exact nonempty

/-- Deleting a link the child does not hold changes nothing. -/
theorem unlink_of_absent_changes_nothing (links : List String) (target : String)
    (absent : target ∉ links) : unlink links target = links := by
  unfold unlink
  split
  · -- Every link differs from the target, so the filter keeps them all.
    apply List.filter_eq_self.mpr
    intro a mem
    have : a ≠ target := fun same => absent (same ▸ mem)
    simpa using this
  · rfl

/-- An update may write the parent field only with the value it holds. -/
def mayRewrite (stored written : Option String) : Bool :=
  stored == written

/-- The parent field after an update request: unchanged either way. -/
def rewrite (stored written : Option String) : Option String :=
  if mayRewrite stored written then written else stored

/-- No update moves a registered document to another parent. -/
theorem field_keeps_admitted_parent (stored written : Option String) :
    rewrite stored written = stored := by
  unfold rewrite mayRewrite
  split
  · -- Allowed means the written value equals the stored one.
    rename_i same
    exact (beq_iff_eq.mp same).symm
  · rfl

/-- The inputs the conformance test resolves on both sides. -/
def resolveCases : Json :=
  let inputs : List Input := [
    ⟨true, "entity_set", "node", "", "a"⟩,
    ⟨true, "entity_set", "node", "", ""⟩,
    ⟨true, "entity_set", "node", "member", "a"⟩,
    ⟨true, "entity", "node", "", "a"⟩,
    ⟨true, "none", "node", "", "a"⟩,
    ⟨false, "entity_set", "node", "", "a"⟩]
  toJson <| inputs.map fun i => Json.mkObj [("input", toJson i), ("parent", toJson (resolve i))]

/-- The delete requests the conformance test runs on both sides. -/
def unlinkCases : Json :=
  let requests : List (List String × String) := [
    (["a"], "a"), (["a"], "b"), (["a", "b"], "a"), (["a", "b"], "b"),
    (["a", "b"], "c"), (["a", "b", "c"], "b")]
  toJson <| requests.map fun (links, target) => Json.mkObj
    [("links", toJson links), ("target", toJson target),
     ("allowed", toJson (mayUnlink links target)), ("after", toJson (unlink links target))]

end ParentRegistration

def main (args : List String) : IO Unit := do
  -- `unlink` prints the delete requests; anything else the resolve inputs.
  if args == ["unlink"] then
    IO.println ParentRegistration.unlinkCases.compress
  else
    IO.println ParentRegistration.resolveCases.compress

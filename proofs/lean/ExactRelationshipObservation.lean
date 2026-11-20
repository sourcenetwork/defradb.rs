import Lean

/-! The story. A caller asks whether one exact actor holds one relation on one
document. Only a caller who may manage that relation may ask. Against Vera the
answer is assembled from certified proofs (the policy, the object's
incarnation, the relationship record), and those proofs must describe one
certified root. Against the local ACP the answer is read in one transaction.
What must never happen: an unauthorized caller learns anything, including
whether the document is registered; proofs from different roots, or about a
different subject, produce a definitive presence or absence.

The code: `exact_relationship::query` in
`crates/engine/vera/src/vera_rs/provider/exact_relationship.rs` (Vera) and
`ZanzibarDocumentACP::observe_actor_relationship` in
`crates/engine/acp/src/zanzibar/acp/document_acp.rs` (local). -/
namespace ExactRelationshipObservation

/-- The evidence and authority needed to observe an exact relationship. -/
structure Input where
  -- The caller owns the document or holds a relation that manages this one.
  authorized : Bool
  -- The policy record was verified against a certified root.
  policyVerified : Bool
  -- The object's incarnation record was verified.
  objectVerified : Bool
  -- The relationship record (or its absence) was verified.
  relationshipVerified : Bool
  -- All three proofs name the same certified root.
  sameRoot : Bool
  -- The relationship key is exactly the requested subject, not a wildcard.
  exactKey : Bool
  -- The verified record is present.
  present : Bool
  -- The verified record is archived (a revoked grant).
  archived : Bool
  deriving Lean.FromJson, Lean.ToJson, Repr

/-- An incomplete or mismatched proof is an error, never an absent grant. -/
def observe (i : Input) : Option Bool :=
  -- Every condition must hold for a definitive answer.
  if i.authorized && i.policyVerified && i.objectVerified && i.relationshipVerified &&
      i.sameRoot && i.exactKey then some (i.present && !i.archived)
  -- Anything else is a refusal, not "absent".
  else none

/-- A caller without management authority cannot observe the tuple. -/
theorem unauthorized (i : Input) (h : i.authorized = false) : observe i = none := by
  -- The first conjunct is false.
  simp [observe, h]

/-- Evidence from different roots cannot establish presence or absence. -/
theorem mismatched_roots (i : Input) (h : i.sameRoot = false) : observe i = none := by
  -- The root conjunct is false.
  simp [observe, h]

/-- A wildcard or another subject is not evidence of the requested tuple. -/
theorem different_subject (i : Input) (h : i.exactKey = false) : observe i = none := by
  -- The exact-key conjunct is false.
  simp [observe, h]

/-- A proven absence cannot be manufactured from an incomplete proof. -/
theorem absence_requires_evidence (i : Input) (h : observe i = some false) :
    i.policyVerified = true ∧ i.objectVerified = true ∧
      i.relationshipVerified = true ∧ i.sameRoot = true ∧ i.exactKey = true := by
  -- A `some` answer comes only from the branch where every condition held.
  simp only [observe] at h
  split at h <;> simp_all

-- Why a local observation is refused, in the order the local ACP checks.
inductive Refusal where
  -- The caller may not manage the relation.
  | notManager
  -- The document has no owner here.
  | notRegistered
  -- The policy does not declare the relation.
  | relationMissing
  deriving DecidableEq, Repr

/-- The local evaluation order: authority first, then registration, then the
relation, then the stored tuple. -/
def observeLocal (authorized registered declared present : Bool) : Except Refusal Bool :=
  -- An unauthorized caller is refused before anything about the document is read.
  if !authorized then .error .notManager
  -- An unregistered document is refused only to an authorized caller.
  else if !registered then .error .notRegistered
  -- So is an undeclared relation.
  else if !declared then .error .relationMissing
  -- Otherwise the stored tuple answers.
  else .ok present

/-- An unauthorized caller gets the same refusal whatever the document's state:
it learns neither registration, nor the relation, nor the tuple. -/
theorem unauthorized_learns_nothing (r₁ d₁ p₁ r₂ d₂ p₂ : Bool) :
    observeLocal false r₁ d₁ p₁ = observeLocal false r₂ d₂ p₂ := by
  -- Both sides take the first branch.
  simp [observeLocal]

/-- A local answer is given only to an authorized caller. -/
theorem local_answer_requires_authority (r d p : Bool) (b : Bool)
    (h : observeLocal false r d p = .ok b) : False := by
  -- The unauthorized branch is an error, never `.ok`.
  simp [observeLocal] at h

end ExactRelationshipObservation

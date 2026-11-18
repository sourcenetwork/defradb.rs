import Lean

/-! Exact relationship observations use certified membership or absence for
the requested subject. Policy, object incarnation, and relationship evidence
must describe one root. A requester must be authorized to manage the relation. -/
namespace ExactRelationshipObservation

/-- The evidence and authority needed to observe an exact relationship. -/
structure Input where
  authorized : Bool
  policyVerified : Bool
  objectVerified : Bool
  relationshipVerified : Bool
  sameRoot : Bool
  exactKey : Bool
  present : Bool
  archived : Bool
  deriving Lean.FromJson, Lean.ToJson, Repr

/-- An incomplete or mismatched proof is an error, never an absent grant. -/
def observe (i : Input) : Option Bool :=
  if i.authorized && i.policyVerified && i.objectVerified && i.relationshipVerified &&
      i.sameRoot && i.exactKey then some (i.present && !i.archived)
  else none

/-- A caller without management authority cannot observe the tuple. -/
theorem unauthorized (i : Input) (h : i.authorized = false) : observe i = none := by
  simp [observe, h]

/-- Evidence from different roots cannot establish presence or absence. -/
theorem mismatched_roots (i : Input) (h : i.sameRoot = false) : observe i = none := by
  simp [observe, h]

/-- A wildcard or another subject is not evidence of the requested tuple. -/
theorem different_subject (i : Input) (h : i.exactKey = false) : observe i = none := by
  simp [observe, h]

/-- A proven absence cannot be manufactured from an incomplete proof. -/
theorem absence_requires_evidence (i : Input) (h : observe i = some false) :
    i.policyVerified = true ∧ i.objectVerified = true ∧
      i.relationshipVerified = true ∧ i.sameRoot = true ∧ i.exactKey = true := by
  simp only [observe] at h
  split at h <;> simp_all

end ExactRelationshipObservation

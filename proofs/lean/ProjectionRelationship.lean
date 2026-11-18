import Lean

/-! A trusted document projector runs inside the transaction that accepted a
document. It may publish a relationship on that document only when the actor
who caused the mutation is authorized for the native relationship operation. It does
not obtain document-read bypass, and it cannot change another document or an
undeclared relationship. A concurrent revocation must invalidate the transaction. -/
namespace ProjectionRelationship
open Lean

/-- A configured projection rule names one resource, relation and target
kind. Native checks the actual relationship against this list. -/
structure Rule where
  collection : String
  resource : String
  relation : String
  targetKind : String
  deriving DecidableEq, FromJson, ToJson, Repr

/-- An operation outside the Node's configured projection rules is refused. -/
def inScope (rules : List Rule) (collection resource relation targetKind : String) : Bool :=
  rules.contains ⟨collection, resource, relation, targetKind⟩

/-- An actorless native callback acts as the Node itself. An authenticated
mutation keeps its actual actor; neither path borrows the document owner. -/
def projectionActor (actor : Option String) (node : String) : String :=
  actor.getD node

theorem actorless_projection_uses_node (node : String) :
    projectionActor none node = node := by rfl

theorem signed_projection_keeps_actor (actor node : String) :
    projectionActor (some actor) node = actor := by rfl

/-- No rule means no projection authority. -/
theorem empty_scope_denied (collection resource relation targetKind : String) :
    inScope [] collection resource relation targetKind = false := by rfl

/-- Native projection cannot escape the explicitly configured rule set. -/
theorem outside_rules_denied (rules : List Rule) (collection resource relation targetKind : String)
    (h : ⟨collection, resource, relation, targetKind⟩ ∉ rules) :
    inScope rules collection resource relation targetKind = false := by
  simp [inScope, h]

/-- The native facts observed in the transaction performing a projection. -/
structure Input where
  projectorContext : Bool
  actorMatches : Bool
  nacActive : Bool
  nacBound : Bool
  operationAllowed : Bool
  dacBound : Bool
  dacAllowed : Bool
  documentMatches : Bool
  reconciliation : Bool
  scopeMatches : Bool
  registered : Bool
  relationValid : Bool
  deriving FromJson, ToJson, Repr

/-- Only a transaction-bound projection of the same registered document
may publish a declared relationship. -/
def mayProject (i : Input) : Bool :=
  i.projectorContext && i.actorMatches && i.nacActive && i.nacBound &&
    i.operationAllowed && i.dacBound && i.dacAllowed && (i.documentMatches || i.reconciliation) && i.scopeMatches &&
    i.registered && i.relationValid

/-- Ordinary requests cannot obtain projector authority. -/
theorem ordinary_request_denied (i : Input) (h : i.projectorContext = false) :
    mayProject i = false := by simp [mayProject, h]

/-- A projector cannot use this authority on a different document. -/
theorem other_document_denied (i : Input) (h : i.documentMatches = false)
    (notReconciliation : i.reconciliation = false) :
    mayProject i = false := by simp [mayProject, h, notReconciliation]

/-- Relationships outside the configured projection scope are refused. -/
theorem outside_scope_denied (i : Input) (h : i.scopeMatches = false) :
    mayProject i = false := by simp [mayProject, h]

/-- A revoked native operation cannot publish the relationship. -/
theorem revoked_operation_denied (i : Input) (h : i.operationAllowed = false) :
    mayProject i = false := by simp [mayProject, h]

/-- The projection cannot borrow a different actor's identity. -/
theorem different_actor_denied (i : Input) (h : i.actorMatches = false) :
    mayProject i = false := by simp [mayProject, h]

/-- A detached authorization decision cannot authorize a write. -/
theorem detached_nac_denied (i : Input) (h : i.nacBound = false) :
    mayProject i = false := by simp [mayProject, h]

/-- Native document authorization and the write share one transaction. -/
theorem detached_dac_denied (i : Input) (h : i.dacBound = false) :
    mayProject i = false := by simp [mayProject, h]

/-- Native document ACP must authorize the actor's relationship change. -/
theorem dac_denied (i : Input) (h : i.dacAllowed = false) :
    mayProject i = false := by simp [mayProject, h]

/-- A document must already have a verified owner. -/
theorem unregistered_denied (i : Input) (h : i.registered = false) :
    mayProject i = false := by simp [mayProject, h]

/-- A policy must declare the relationship and accept its target. -/
theorem invalid_relation_denied (i : Input) (h : i.relationValid = false) :
    mayProject i = false := by simp [mayProject, h]

/-- Executable decisions drive the native conformance test. -/
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun i => Json.mkObj
    [("input", toJson i), ("mayProject", toJson (mayProject i))]

/-- These cases exercise matching and refusal across the configured scope. -/
def scopeCases : Json :=
  let rules : List Rule := [⟨"Request", "enrollment_records", "reader", "entity"⟩,
    ⟨"Request", "enrollment_records", "parent", "entity_set"⟩,
    ⟨"PeerRegistry", "discovery", "reader", "wildcard"⟩]
  let candidates : List Rule := [⟨"Request", "enrollment_records", "reader", "entity"⟩,
    ⟨"Request", "enrollment_records", "reader", "wildcard"⟩,
    ⟨"Request", "enrollment_records", "parent", "entity_set"⟩,
    ⟨"PeerRegistry", "discovery", "reader", "wildcard"⟩,
    ⟨"NodeDirectoryEntry", "discovery", "reader", "wildcard"⟩,
    ⟨"Session", "sessions", "reader", "entity"⟩,
    ⟨"Request", "enrollment_records", "writer", "entity"⟩]
  toJson <| candidates.map fun candidate => Json.mkObj
    [("rules", toJson rules), ("candidate", toJson candidate),
     ("allowed", toJson (inScope rules candidate.collection candidate.resource candidate.relation candidate.targetKind))]

end ProjectionRelationship

def main (args : List String) : IO Unit := do
  if args == ["scope"] then
    IO.println ProjectionRelationship.scopeCases.compress
  else
    let input ← (← IO.getStdin).getLine
    match Lean.Json.parse input >>= ProjectionRelationship.evaluate with
    | .ok output => IO.println output.compress
    | .error error => throw <| IO.userError error

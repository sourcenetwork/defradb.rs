import Lean

/-!
A document owner can manage its relationships. A declared manager relation
can also manage the relations it names: its native Zanzibar evaluation may
match a direct actor, a wildcard, or a userset on the exact physical Node.
An unrelated Node or a manager for a different target is not authority.
The native engine is responsible for bounded, cycle-safe traversal and the
transaction validates every read when it commits.
-/
namespace UsersetRelationManager
open Lean

-- Facts returned by the native store and its userset evaluator.
structure Input where
  owner : Bool
  declared : Bool
  direct : Bool
  wildcard : Bool
  sameNode : Bool
  nodeOwner : Bool
  nodeAdmin : Bool
  deriving FromJson, ToJson

-- Only this target's declared manager may use membership in the exact Node.
def mayManage (i : Input) : Bool :=
  i.owner || (i.declared && (i.direct || i.wildcard ||
    (i.sameNode && (i.nodeOwner || i.nodeAdmin))))

-- Document ownership remains sufficient.
theorem owner_allowed (i : Input) (h : i.owner = true) : mayManage i = true := by
  simp [mayManage, h]

-- A manager relation cannot manage a target it does not declare.
theorem undeclared_denied (i : Input) (owner : i.owner = false) (declared : i.declared = false) :
    mayManage i = false := by simp [mayManage, owner, declared]

-- A userset on another Node cannot grant manager authority.
theorem foreign_node_denied (i : Input) (owner : i.owner = false) (direct : i.direct = false)
    (wildcard : i.wildcard = false) (same : i.sameNode = false) :
    mayManage i = false := by simp [mayManage, owner, direct, wildcard, same]

-- A direct manager keeps its existing native authority.
theorem direct_allowed (i : Input) (declared : i.declared = true) (direct : i.direct = true) :
    mayManage i = true := by simp [mayManage, declared, direct]

-- A wildcard manager keeps its existing native authority.
theorem wildcard_allowed (i : Input) (declared : i.declared = true) (wildcard : i.wildcard = true) :
    mayManage i = true := by simp [mayManage, declared, wildcard]

-- The target Node's owner is a member of its owner userset.
theorem node_owner_allowed (i : Input) (declared : i.declared = true)
    (same : i.sameNode = true) (owner : i.nodeOwner = true) :
    mayManage i = true := by simp [mayManage, declared, same, owner]

-- A current administrator is a member of that Node's administrator userset.
theorem node_admin_allowed (i : Input) (declared : i.declared = true)
    (same : i.sameNode = true) (admin : i.nodeAdmin = true) :
    mayManage i = true := by simp [mayManage, declared, same, admin]

-- Conformance consumes the same facts the native transaction test exercises.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun i => mayManage i
end UsersetRelationManager

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= UsersetRelationManager.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error

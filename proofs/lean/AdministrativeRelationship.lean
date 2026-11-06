import Lean

/-! A native node administrator may manage a document relationship that
belongs to another actor. The capability is constructed inside the transaction
from the acting DID's active NAC `bypass-dac` and add/delete permission; it is
not a user identity, a caller-supplied bypass bit, or an ACP grant. NAC and DAC
must share the transaction, whose commit validates its authorization reads. -/
namespace AdministrativeRelationship
open Lean

/-- Facts read by the transaction-bound administrative relationship owner. -/
structure Input where
  actorMatches : Bool
  nacActive : Bool
  nacBound : Bool
  bypassAllowed : Bool
  operationAllowed : Bool
  dacBound : Bool
  registered : Bool
  relationValid : Bool
  deriving FromJson, ToJson

/-- The actual actor can mutate only after every native premise is satisfied. -/
def mayAdmin (i : Input) : Bool :=
  i.actorMatches && i.nacActive && i.nacBound && i.bypassAllowed &&
    i.operationAllowed && i.dacBound && i.registered && i.relationValid

/-- Disabled NAC is never an administrator capability. -/
theorem inactive_denied (i : Input) (h : i.nacActive = false) : mayAdmin i = false := by
  simp [mayAdmin, h]

/-- A caller without native bypass authority cannot manage a foreign document. -/
theorem bypass_denied (i : Input) (h : i.bypassAllowed = false) : mayAdmin i = false := by
  simp [mayAdmin, h]

/-- A bypass grant alone does not authorize relation add or delete. -/
theorem operation_denied (i : Input) (h : i.operationAllowed = false) : mayAdmin i = false := by
  simp [mayAdmin, h]

/-- A detached NAC proof cannot authorize this transaction. -/
theorem unbound_nac_denied (i : Input) (h : i.nacBound = false) : mayAdmin i = false := by
  simp [mayAdmin, h]

/-- An unsupported or detached DAC provider must refuse the operation. -/
theorem unbound_dac_denied (i : Input) (h : i.dacBound = false) : mayAdmin i = false := by
  simp [mayAdmin, h]

/-- Conformance runs the executable owner on native adapter inputs. -/
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun i => Json.mkObj
    [("input", toJson i), ("mayAdmin", toJson (mayAdmin i))]
end AdministrativeRelationship

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= AdministrativeRelationship.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error

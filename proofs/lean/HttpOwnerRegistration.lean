import Lean

namespace HttpOwnerRegistration
open Lean

/-- The HTTP boundary receives the native transaction's visibility and owner
observation. It reveals no owner identity, and a hidden or absent document does
not disclose whether an owner is registered. -/
structure Input where
  nativeVisible : Bool
  ownerPresent : Bool
  callerIsOwner : Bool
  deriving FromJson, ToJson

def registered (i : Input) : Bool := i.nativeVisible && i.ownerPresent

def isOwner (i : Input) : Bool := registered i && i.callerIsOwner

theorem hidden_does_not_disclose (i : Input) (h : i.nativeVisible = false) :
    registered i = false ∧ isOwner i = false := by simp [registered, isOwner, h]

theorem caller_owner_implies_registered (i : Input) :
    isOwner i = true → registered i = true := by
  cases h : registered i <;> simp [isOwner, h]

def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun i => Json.mkObj
    [("input", toJson i), ("visible", toJson i.nativeVisible),
     ("owner_registered", toJson (registered i)), ("is_owner", toJson (isOwner i))]
end HttpOwnerRegistration

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= HttpOwnerRegistration.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error

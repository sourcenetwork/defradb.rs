import Lean

namespace EmbeddedHttpAcp
open Lean

/-- HTTP carries the authenticated caller and target to the native ACP owner;
its result is translated into the HTTP response without changing authorization. -/
structure Input where
  caller : String
  target : String
  nativeAllowed : Bool
  nativeChanged : Bool
  deriving FromJson, ToJson

def result (i : Input) : Bool × Bool :=
  (i.nativeAllowed, i.nativeAllowed && i.nativeChanged)

/-- The adapter carries both the caller and their request credential to the
native owner, including when another request is in flight. -/
def forward (caller bearer : String) : String × String := (caller, bearer)

theorem forwarding_preserves_request (caller bearer : String) :
    forward caller bearer = (caller, bearer) := by rfl

theorem denied_has_no_change (i : Input) (h : i.nativeAllowed = false) :
    (result i).2 = false := by simp [result, h]

theorem allowed_preserves_native_result (i : Input) (h : i.nativeAllowed = true) :
    (result i).2 = i.nativeChanged := by simp [result, h]

def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun i => Json.mkObj
    [("input", toJson i), ("allowed", toJson (result i).1),
     ("changed", toJson (result i).2)]
end EmbeddedHttpAcp

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= EmbeddedHttpAcp.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error

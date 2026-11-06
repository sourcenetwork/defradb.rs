import HostMutationProjection
import Lean

open Lean

def evaluateHostSources (json : Json) : Except String Json := do
  let inputs : Array HostMutationProjection.HostSourceAdmission ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input),
     ("allowed", toJson (HostMutationProjection.hostSourceAllowed input))]

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= evaluateHostSources with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error

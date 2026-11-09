import Lean

/-! Delete selection admits a single physical ID and no additional predicate.
The native DeleteNode does not retain additional filters after resolving IDs. -/
namespace ClientSignedDeleteFilter
open Lean
structure Input where
  expected : String
  direct : Option String
  filter : Json
  deriving FromJson, ToJson

def accepts (input : Input) : Bool :=
  if input.filter.isNull then input.direct == some input.expected
  else input.direct.isNone && input.filter == Json.mkObj [("_docID", Json.mkObj [("_eq", toJson input.expected)])]

def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("accepted", toJson (accepts input))]
end ClientSignedDeleteFilter

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ClientSignedDeleteFilter.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error

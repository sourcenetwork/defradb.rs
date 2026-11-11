import Lean

/-!
A client-signed conditional update may select only the approved physical
DocID. A top-level equality on `_docID` is a conjunction with the remaining
filter; DefraDB evaluates the complete predicate and validates the fields it
reads in the transaction. Filter evaluation and transactional conflict
validation are native premises.
-/
namespace ClientSignedUpdateFilter
open Lean

structure Input where
  expected : String
  direct : Option String
  filter : Json
  deriving FromJson, ToJson

def accepts (input : Input) : Bool :=
  if input.filter.isNull then
    input.direct == some input.expected
  else
    input.direct.isNone && input.filter.getObjValD "_docID" ==
      Json.mkObj [("_eq", toJson input.expected)]

theorem accepted_filter_has_approved_target (input : Input)
    (hasFilter : input.filter.isNull = false) (accepted : accepts input = true) :
    (input.filter.getObjValD "_docID" == Json.mkObj [("_eq", toJson input.expected)]) = true := by
  simp [accepts, hasFilter] at accepted
  exact accepted.2

def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("accepted", toJson (accepts input))]
end ClientSignedUpdateFilter

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ClientSignedUpdateFilter.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error

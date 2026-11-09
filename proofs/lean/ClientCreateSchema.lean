import Lean

/-!
A create can prepare signed blocks before its transaction runs. A collection
can keep its version ID while changing a field's immutable flag, which changes
the blocks the document requires. A cached graph is reusable only when the
complete deciding definition still matches, not merely its version ID. This
model isolates version and ordered immutable field flags; the Rust owner checks
the full collection definition. Rust owner: crates/db/src/write/create.rs.
-/

namespace ClientCreateSchema
open Lean

-- A modeled field has a name and its immutable declaration.
structure Field where
  name : String
  immutable : Bool
  deriving DecidableEq, FromJson, ToJson

-- The deciding portion of a trusted schema includes the version and flags.
structure Definition where
  version : String
  fields : List Field
  deriving DecidableEq, FromJson, ToJson

-- Reuse requires the same modeled definition, including every field flag.
def reusable (prepared current : Definition) : Bool := decide (prepared = current)

-- An accepted cached graph cannot cross a change of immutable field flags.
theorem reuse_requires_same_definition (prepared current : Definition)
    (valid : reusable prepared current = true) : prepared = current := by
  simpa [reusable] using valid

-- Conformance receives actual before and after definitions.
structure Input where
  prepared : Definition
  current : Definition
  deriving FromJson, ToJson

-- Expected decisions come from the executable model.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("reusable", toJson (reusable input.prepared input.current))]

end ClientCreateSchema

-- A native regression sends the actual schemas as JSON to this model.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ClientCreateSchema.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error

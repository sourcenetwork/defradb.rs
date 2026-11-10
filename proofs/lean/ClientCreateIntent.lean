import Lean

/-!
A create's approved intent is the ordered list of its explicitly present
fields and their schema-normalized JSON values. Null is a present value, not
an omitted field. Generated values are rejected before conversion. Native
schema conversion supplies the canonical values; this model does not prove
that conversion or its encoding.
-/
namespace ClientCreateIntent
open Lean

structure Field where
  name : String
  value : String
  deriving DecidableEq, FromJson, ToJson

def accepts (approved offered : List Field) (generated : Bool) : Bool :=
  !generated && decide (approved = offered)

theorem accepted_exact (approved offered : List Field) (generated : Bool)
    (accepted : accepts approved offered generated = true) : approved = offered := by
  exact (show generated = false ∧ approved = offered by simpa [accepts] using accepted).2

theorem generated_is_rejected (approved offered : List Field) :
    accepts approved offered true = false := by
  simp [accepts]

theorem null_cannot_be_omitted (approved offered : List Field) (name : String) (generated : Bool)
    (present : ⟨name, "null"⟩ ∈ approved)
    (absent : ∀ field ∈ offered, field.name ≠ name) : accepts approved offered generated = false := by
  apply Bool.eq_false_iff.mpr
  intro accepted
  have same := accepted_exact approved offered generated accepted
  subst offered
  exact (absent ⟨name, "null"⟩ present) rfl

structure Input where
  approved : List Field
  offered : List Field
  generated : Bool
  deriving FromJson, ToJson

def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("accepted", toJson (accepts input.approved input.offered input.generated))]
end ClientCreateIntent

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ClientCreateIntent.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error

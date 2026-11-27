import Lean

/-!
A projector stages an existing document invalidation when it changed the
relationship. The storage callback owner publishes it only after commit;
an aborted transaction discards both the relationship and its callback.
An invalidation asks a current-state observer to re-read with native ACP.
It does not promise a removal event on GraphQL's filtered SSE stream.
-/
namespace ProjectionAccessNotification

-- A changed grant or revoke needs a callback only when its transaction commits.
def notify (before after committed : Bool) : Bool :=
  committed && (before != after)

-- An unchanged relationship does not publish an invalidation.
theorem unchanged_does_not_notify (value committed : Bool) :
    notify value value committed = false := by
  cases value <;> cases committed <;> rfl

-- An abort never publishes an invalidation.
theorem abort_does_not_notify (before after : Bool) :
    notify before after false = false := by
  cases before <;> cases after <;> rfl

-- A committed grant wakes the current-state observer.
theorem committed_grant_notifies : notify false true true = true := by rfl

-- A committed revoke wakes the current-state observer as well.
theorem committed_revoke_notifies : notify true false true = true := by rfl

open Lean

-- Inputs describe one native relation attempt and its transaction result.
structure Input where
  before : Bool
  after : Bool
  committed : Bool
  deriving FromJson

-- Conformance uses the model's result for each actual native operation.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun i => notify i.before i.after i.committed

end ProjectionAccessNotification

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ProjectionAccessNotification.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
